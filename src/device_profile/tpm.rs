//! TPM 2.0 vendor, firmware and endorsement-key fingerprint.
//!
//! Two raw TPM commands are issued: `GetCapability` for the fixed properties
//! (vendor, firmware, spec version) and `ReadPublic` of the endorsement key at
//! its standard persistent handle. The EK public key is reported as the SHA-256
//! of its DER `SubjectPublicKeyInfo` — the canonical form the server pins.
//! Because the client does the hashing, a divergent encoding cannot be repaired
//! server-side, so the SPKI layout here is verified against OpenSSL fixtures.
//!
//! Every field is read with bounds checks; any shortfall yields no value rather
//! than a panic or a guess. The transport is TBS on Windows and `/dev/tpmrm0`
//! on Linux; macOS has no TPM. Reading usually needs no elevation on Windows,
//! and root or the `tss` group on Linux, so most Linux installs send nothing.

use super::Builder;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const CC_GET_CAPABILITY: u32 = 0x0000_017A;
const CC_READ_PUBLIC: u32 = 0x0000_0173;
const ST_NO_SESSIONS: u16 = 0x8001;
const CAP_TPM_PROPERTIES: u32 = 0x0000_0006;
const PT_FIXED: u32 = 0x0000_0100;
const PT_FAMILY_INDICATOR: u32 = 0x0000_0100;
const PT_MANUFACTURER: u32 = 0x0000_0105;
const PT_FIRMWARE_VERSION_1: u32 = 0x0000_010B;
const PT_FIRMWARE_VERSION_2: u32 = 0x0000_010C;
const EK_RSA_HANDLE: u32 = 0x8101_0001;
const EK_ECC_HANDLE: u32 = 0x8101_0002;
const ALG_RSA: u16 = 0x0001;
const ALG_ECC: u16 = 0x0023;
const ALG_NULL: u16 = 0x0010;

/// The TCG TPM Vendor ID Registry (product implementations), four-character id
/// to assignee. A factual table; unknown ids keep only the raw id.
const TCG_VENDORS: &[(&str, &str)] = &[
    ("AMD", "AMD"),
    ("ANT", "Ant Group"),
    ("ATML", "Atmel"),
    ("BRCM", "Broadcom"),
    ("CSCO", "Cisco"),
    ("FLYS", "Flyslice Technologies"),
    ("ROCC", "Fuzhou Rockchip"),
    ("GOOG", "Google"),
    ("HPI", "HPI"),
    ("HPE", "HPE"),
    ("HISI", "Huawei"),
    ("IBM", "IBM"),
    ("IFX", "Infineon"),
    ("INTC", "Intel"),
    ("LEN", "Lenovo"),
    ("MSFT", "Microsoft"),
    ("NSM", "National Semiconductor"),
    ("NTZ", "Nationz"),
    ("NTC", "Nuvoton"),
    ("QCOM", "Qualcomm"),
    ("SECE", "Secure Edge"),
    ("SMSC", "SMSC"),
    ("STM", "STMicroelectronics"),
    ("SMSN", "Samsung"),
    ("SNS", "Sinosun"),
    ("TXN", "Texas Instruments"),
    ("WEC", "Winbond"),
];

fn tcg_vendor(id: &str) -> Option<&'static str> {
    TCG_VENDORS
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, name)| *name)
}

pub(super) fn collect(b: &mut Builder) {
    let Some(tpm) = Transport::open() else {
        return;
    };
    if let Some(props) = get_capability(&tpm) {
        apply_properties(b, &props);
    }
    // RSA template first: it is the most universally provisioned, which is what
    // the server asked for, to minimise where two clients could diverge.
    for (handle, alg) in [(EK_RSA_HANDLE, "rsa2048"), (EK_ECC_HANDLE, "ecc-p256")] {
        if let Some(key) = read_public(&tpm, handle) {
            let digest = Sha256::digest(spki_der(&key));
            b.put("device.tpm_ek", Some(hex(&digest)));
            b.put("device.tpm_ek_alg", Some(alg));
            break;
        }
    }
}

fn apply_properties(b: &mut Builder, props: &BTreeMap<u32, u32>) {
    if let Some(s) = props.get(&PT_FAMILY_INDICATOR).copied().and_then(ascii4) {
        b.put("device.tpm_spec_version", Some(s));
    }
    if let Some(id) = props.get(&PT_MANUFACTURER).copied().and_then(ascii4) {
        b.put("device.tpm_vendor", Some(id.clone()));
        if let Some(name) = tcg_vendor(&id) {
            b.put("device.tpm_vendor_name", Some(name));
        }
    }
    if let (Some(&v1), Some(&v2)) = (
        props.get(&PT_FIRMWARE_VERSION_1),
        props.get(&PT_FIRMWARE_VERSION_2),
    ) {
        b.put(
            "device.tpm_firmware_version",
            Some(format!(
                "{}.{}.{}.{}",
                v1 >> 16,
                v1 & 0xffff,
                v2 >> 16,
                v2 & 0xffff
            )),
        );
    }
}

/// Four big-endian bytes as a printable-ASCII string (TPM packs `"2.0\0"` and
/// `"IFX\0"` this way). None if any non-terminating byte is not printable.
fn ascii4(v: u32) -> Option<String> {
    let mut s = String::new();
    for &c in &v.to_be_bytes() {
        if c == 0 {
            break;
        }
        if !(0x20..=0x7e).contains(&c) {
            return None;
        }
        s.push(c as char);
    }
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ------------------------------------------------------------- TPM commands ---

fn get_capability(tpm: &Transport) -> Option<BTreeMap<u32, u32>> {
    let mut body = Vec::new();
    body.extend_from_slice(&CAP_TPM_PROPERTIES.to_be_bytes());
    body.extend_from_slice(&PT_FIXED.to_be_bytes());
    body.extend_from_slice(&64u32.to_be_bytes()); // propertyCount
    let resp = tpm.transmit(&command(CC_GET_CAPABILITY, &body))?;
    let mut c = Cursor::after_header(&resp)?;
    let _more = c.u8()?;
    let _cap = c.u32()?;
    let count = c.u32()?;
    let mut out = BTreeMap::new();
    for _ in 0..count {
        let property = c.u32()?;
        let value = c.u32()?;
        out.insert(property, value);
    }
    Some(out)
}

fn read_public(tpm: &Transport, handle: u32) -> Option<PublicKey> {
    let resp = tpm.transmit(&command(CC_READ_PUBLIC, &handle.to_be_bytes()))?;
    let mut c = Cursor::after_header(&resp)?;
    let public_area = c.tpm2b()?; // outPublic: TPM2B_PUBLIC
    parse_public_area(public_area)
}

/// Build a command: tag (no sessions), total size, command code, then body.
fn command(code: u32, body: &[u8]) -> Vec<u8> {
    let size = 10 + body.len() as u32;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(&ST_NO_SESSIONS.to_be_bytes());
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(&code.to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// A TPMT_PUBLIC public area to the public key it carries.
fn parse_public_area(area: &[u8]) -> Option<PublicKey> {
    let mut c = Cursor::new(area);
    let kind = c.u16()?;
    let _name_alg = c.u16()?;
    let _attributes = c.u32()?;
    let _auth_policy = c.tpm2b()?;
    match kind {
        ALG_RSA => {
            // TPMS_RSA_PARMS
            if c.u16()? != ALG_NULL {
                c.u16()?; // symmetric keyBits
                c.u16()?; // symmetric mode
            }
            if c.u16()? != ALG_NULL {
                c.u16()?; // scheme hashAlg
            }
            let _key_bits = c.u16()?;
            let exponent = match c.u32()? {
                0 => 65537,
                e => e,
            };
            let modulus = c.tpm2b()?.to_vec();
            Some(PublicKey::Rsa { modulus, exponent })
        }
        ALG_ECC => {
            // TPMS_ECC_PARMS
            if c.u16()? != ALG_NULL {
                c.u16()?; // symmetric keyBits
                c.u16()?; // symmetric mode
            }
            if c.u16()? != ALG_NULL {
                c.u16()?; // scheme hashAlg
            }
            let _curve = c.u16()?;
            if c.u16()? != ALG_NULL {
                c.u16()?; // kdf hashAlg
            }
            let x = c.tpm2b()?.to_vec();
            let y = c.tpm2b()?.to_vec();
            Some(PublicKey::Ecc { x, y })
        }
        _ => None,
    }
}

enum PublicKey {
    Rsa { modulus: Vec<u8>, exponent: u32 },
    Ecc { x: Vec<u8>, y: Vec<u8> },
}

/// A big-endian reader over a byte slice; every read is bounds-checked.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    /// Positioned just past a TPM response header (tag, size, responseCode),
    /// or None when the header is short or the response code is not success.
    fn after_header(data: &'a [u8]) -> Option<Self> {
        let mut c = Cursor::new(data);
        let _tag = c.u16()?;
        let _size = c.u32()?;
        if c.u32()? != 0 {
            return None;
        }
        Some(c)
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A TPM2B: a u16 length then that many bytes.
    fn tpm2b(&mut self) -> Option<&'a [u8]> {
        let len = self.u16()? as usize;
        self.take(len)
    }
}

// --------------------------------------------------------------- SPKI (DER) ---

const OID_RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
const OID_PRIME256V1: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];

/// The DER `SubjectPublicKeyInfo` for a public key, matching what OpenSSL emits.
fn spki_der(key: &PublicKey) -> Vec<u8> {
    match key {
        PublicKey::Rsa { modulus, exponent } => {
            let alg = der_seq(&[der_oid(OID_RSA), der_null()]);
            let rsa_key = der_seq(&[der_integer(modulus), der_integer(&exponent.to_be_bytes())]);
            der_seq(&[alg, der_bit_string(&rsa_key)])
        }
        PublicKey::Ecc { x, y } => {
            let alg = der_seq(&[der_oid(OID_EC_PUBLIC_KEY), der_oid(OID_PRIME256V1)]);
            let mut point = vec![0x04];
            point.extend_from_slice(&left_pad(x, 32));
            point.extend_from_slice(&left_pad(y, 32));
            der_seq(&[alg, der_bit_string(&point)])
        }
    }
}

fn left_pad(bytes: &[u8], len: usize) -> Vec<u8> {
    let trimmed: &[u8] = {
        let mut s = bytes;
        while s.len() > len && s[0] == 0 {
            s = &s[1..];
        }
        s
    };
    if trimmed.len() >= len {
        return trimmed[trimmed.len() - len..].to_vec();
    }
    let mut out = vec![0u8; len - trimmed.len()];
    out.extend_from_slice(trimmed);
    out
}

fn der_len(n: usize) -> Vec<u8> {
    if n < 0x80 {
        return vec![n as u8];
    }
    let bytes = n.to_be_bytes();
    let start = bytes
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(bytes.len() - 1);
    let significant = &bytes[start..];
    let mut out = vec![0x80 | significant.len() as u8];
    out.extend_from_slice(significant);
    out
}

fn der_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend_from_slice(&der_len(content.len()));
    out.extend_from_slice(content);
    out
}

fn der_seq(parts: &[Vec<u8>]) -> Vec<u8> {
    der_tlv(0x30, &parts.concat())
}

fn der_oid(body: &[u8]) -> Vec<u8> {
    der_tlv(0x06, body)
}

fn der_null() -> Vec<u8> {
    vec![0x05, 0x00]
}

fn der_bit_string(content: &[u8]) -> Vec<u8> {
    let mut with_pad = vec![0x00]; // zero unused bits
    with_pad.extend_from_slice(content);
    der_tlv(0x03, &with_pad)
}

/// A DER INTEGER: leading zero bytes stripped, a single 0x00 prepended when the
/// top bit is set so the value stays positive.
fn der_integer(bytes: &[u8]) -> Vec<u8> {
    let mut s = bytes;
    while s.len() > 1 && s[0] == 0 {
        s = &s[1..];
    }
    let mut content = Vec::new();
    if s.first().is_some_and(|b| b & 0x80 != 0) {
        content.push(0);
    }
    content.extend_from_slice(s);
    der_tlv(0x02, &content)
}

// ---------------------------------------------------------------- transport ---

struct Transport {
    #[cfg(windows)]
    ctx: *mut core::ffi::c_void,
    #[cfg(target_os = "linux")]
    file: std::fs::File,
}

impl Transport {
    #[cfg(windows)]
    fn open() -> Option<Self> {
        use windows::Win32::System::TpmBaseServices::{Tbsi_Context_Create, TBS_CONTEXT_PARAMS2};
        // Version 2, with includeTpm20 set in the bitfield union.
        let mut params = TBS_CONTEXT_PARAMS2 {
            version: 2,
            ..Default::default()
        };
        params.Anonymous.asUINT32 = 0b0100; // includeTpm20
        let mut ctx: *mut core::ffi::c_void = std::ptr::null_mut();
        let rc = unsafe { Tbsi_Context_Create(std::ptr::addr_of!(params).cast(), &mut ctx) };
        (rc == 0 && !ctx.is_null()).then_some(Transport { ctx })
    }

    #[cfg(windows)]
    fn transmit(&self, cmd: &[u8]) -> Option<Vec<u8>> {
        use windows::Win32::System::TpmBaseServices::{
            Tbsip_Submit_Command, TBS_COMMAND_LOCALITY_ZERO, TBS_COMMAND_PRIORITY_NORMAL,
        };
        let mut resp = vec![0u8; 4096];
        let mut len = resp.len() as u32;
        let rc = unsafe {
            Tbsip_Submit_Command(
                self.ctx,
                TBS_COMMAND_LOCALITY_ZERO,
                TBS_COMMAND_PRIORITY_NORMAL,
                cmd,
                resp.as_mut_ptr(),
                &mut len,
            )
        };
        if rc != 0 {
            return None;
        }
        resp.truncate(len as usize);
        Some(resp)
    }

    #[cfg(target_os = "linux")]
    fn open() -> Option<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tpmrm0")
            .ok()?;
        Some(Transport { file })
    }

    #[cfg(target_os = "linux")]
    fn transmit(&self, cmd: &[u8]) -> Option<Vec<u8>> {
        use std::io::{Read, Write};
        let mut file = &self.file;
        file.write_all(cmd).ok()?;
        let mut resp = vec![0u8; 4096];
        let n = file.read(&mut resp).ok()?;
        resp.truncate(n);
        Some(resp)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn open() -> Option<Self> {
        None
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn transmit(&self, _cmd: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(windows)]
impl Drop for Transport {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::TpmBaseServices::Tbsip_Context_Close(self.ctx) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    // Fixtures from Python `cryptography` SubjectPublicKeyInfo, independent of
    // this encoder (see docs/DEVICE_FINGERPRINTING_PLAN.md).
    const RSA_MOD: &str = "b49f50950d67df01212ec9e569e8c82a8b8bf8d5d12f5169abcb5cc473bd8735e99671202db53b2097a566a6f234633c7c25715aaf9a649fb855f4051bccb90f6d9fcf6c25cf256e216054ddfaddc394816bb5130a6c24350c1af0e2ab370480f2d2b1a87ce51abf34ad14cd4a4775497caf38b04442c37a67bec8f88c8505477cc44b3fa9060eda16ea1605c023b646db717cb75ccbc971addd330e314151aab21b988d7371598121d26c73dd7002127d972dd1f8d3decd8f81bb4bbacb90bc48e4d18a332392db335422fc8469ef5c3d1acbbc2954bc23a250d32750f661476e6e7d7fbc00655726950f7c8742de3b50425ee3b45dad259d021ca8b43786bf";
    const RSA_SPKI_SHA256: &str =
        "4b9eafcb46ce3282e4f0a1939ec4fbbd496e2cffe8aa47926234d03f419996ad";
    const ECC_X: &str = "0a46cae0f782c10e9f01e0e0ead139b026ae1e38d37a7f0674d06cf9754d39a7";
    const ECC_Y: &str = "21b9048f8f38d238efb2c5ec7ab04b678ed4ee63e7e18cf3d3e567f860816b4e";
    const ECC_SPKI_SHA256: &str =
        "20a89ff279656af35cd166607082fdb003d2ea67015bc9dcc5e38dbc131de294";

    #[test]
    fn rsa_spki_matches_openssl() {
        let key = PublicKey::Rsa {
            modulus: unhex(RSA_MOD),
            exponent: 65537,
        };
        assert_eq!(hex(&Sha256::digest(spki_der(&key))), RSA_SPKI_SHA256);
    }

    #[test]
    fn ecc_spki_matches_openssl() {
        let key = PublicKey::Ecc {
            x: unhex(ECC_X),
            y: unhex(ECC_Y),
        };
        assert_eq!(hex(&Sha256::digest(spki_der(&key))), ECC_SPKI_SHA256);
    }

    #[test]
    fn der_integer_prepends_zero_for_high_bit() {
        assert_eq!(der_integer(&[0x80]), vec![0x02, 0x02, 0x00, 0x80]);
        assert_eq!(der_integer(&[0x7f]), vec![0x02, 0x01, 0x7f]);
        assert_eq!(der_integer(&[0x00, 0x00, 0x01]), vec![0x02, 0x01, 0x01]);
        // exponent 65537 big-endian is 00 01 00 01 -> 01 00 01
        assert_eq!(
            der_integer(&65537u32.to_be_bytes()),
            vec![0x02, 0x03, 0x01, 0x00, 0x01]
        );
    }

    #[test]
    fn der_len_long_form() {
        assert_eq!(der_len(0x7f), vec![0x7f]);
        assert_eq!(der_len(0x80), vec![0x81, 0x80]);
        assert_eq!(der_len(294), vec![0x82, 0x01, 0x26]);
    }

    #[test]
    fn ascii4_decodes_packed_strings() {
        assert_eq!(
            ascii4(u32::from_be_bytes([0x32, 0x2e, 0x30, 0x00])).as_deref(),
            Some("2.0")
        );
        assert_eq!(
            ascii4(u32::from_be_bytes(*b"IFX\0")).as_deref(),
            Some("IFX")
        );
        assert_eq!(ascii4(0), None);
        assert_eq!(ascii4(u32::from_be_bytes([0xff, 0x00, 0x00, 0x00])), None);
    }

    #[test]
    fn parse_rsa_public_area_finds_modulus() {
        // Minimal TPMT_PUBLIC: RSA, nameAlg, attrs, empty authPolicy, RSA parms
        // with AES-128-CFB symmetric, NULL scheme, 2048 bits, exponent 0, then a
        // 4-byte unique modulus.
        let mut a = Vec::new();
        a.extend_from_slice(&ALG_RSA.to_be_bytes());
        a.extend_from_slice(&0x000bu16.to_be_bytes()); // nameAlg SHA256
        a.extend_from_slice(&0u32.to_be_bytes()); // attributes
        a.extend_from_slice(&0u16.to_be_bytes()); // authPolicy length 0
        a.extend_from_slice(&0x0006u16.to_be_bytes()); // sym AES
        a.extend_from_slice(&128u16.to_be_bytes()); // sym keyBits
        a.extend_from_slice(&0x0043u16.to_be_bytes()); // sym mode CFB
        a.extend_from_slice(&ALG_NULL.to_be_bytes()); // scheme NULL
        a.extend_from_slice(&2048u16.to_be_bytes()); // keyBits
        a.extend_from_slice(&0u32.to_be_bytes()); // exponent 0 -> 65537
        a.extend_from_slice(&4u16.to_be_bytes()); // unique length
        a.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        match parse_public_area(&a) {
            Some(PublicKey::Rsa { modulus, exponent }) => {
                assert_eq!(modulus, vec![0xde, 0xad, 0xbe, 0xef]);
                assert_eq!(exponent, 65537);
            }
            _ => panic!("expected RSA key"),
        }
    }

    #[test]
    fn short_response_yields_none() {
        assert!(Cursor::after_header(&[0x80, 0x01]).is_none());
        // header with a nonzero response code
        let bad = [0x80, 0x01, 0, 0, 0, 10, 0, 0, 0, 1];
        assert!(Cursor::after_header(&bad).is_none());
    }
}
