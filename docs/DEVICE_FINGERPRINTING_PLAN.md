# Advanced device fingerprinting: implementation plan

Status: **planned**, not started. Target release: **7.4.0**. A new class of
signal is a minor version, not a patch.

This plan extends the device profile that `ironmon serve` sends once per session
(`src/device_profile.rs`). It records the decisions already made, the work in
order, and the checks that must pass before release.

## Decisions on record

| Decision | Made by | Notes |
|---|---|---|
| Add posture **and** identity signals, including a TPM endorsement-key hash | Owner (2026-11-21) | IronForensics advised against the EK hash and the peripheral serials. That advice was heard and overruled. |
| Device profile becomes **opt-in in the EU/EEA**, and stays on by default elsewhere | Owner (2026-11-21) | Prompted by ePrivacy Art. 5(3). This is a heuristic, and the privacy review settles it. |
| `device.tpm_ek` canonical form | nervosys-v2 server (9dd3e4e) | See [TPM endorsement key](#tpm-endorsement-key). The client hashes it, so the server cannot repair a divergent form. |
| New identity keys stay inert until they are on the server allowlist | nervosys-v2 server | Order: propose, agree the form, server ships the allowlist entry, **then** release. |
| No code dependency on IronForensics | Circumstance | IronForensics is unpublished and has no remote. Both crates are AGPL-3.0, so the licence is not the obstacle. IronMonitor implements the parsers and registries it needs itself. |
| IronForensics supplies no identity matching | IronForensics | Its rule: identity is asserted, never inferred. It can reason over a profile, and it withholds identifiers by default. |

## New keys

All keys are under `device.`. **Identity** means the key is proposed for the
server's identity allowlist. **Profile** means it is stored in the profile only.

| Key | Type | Kind | Windows | Linux | macOS |
|---|---|---|---|---|---|
| `firmware_type` | `uefi` \| `bios` | Profile | `GetFirmwareType` | `/sys/firmware/efi` present | omit |
| `secure_boot` | bool | Profile | `HKLM\…\SecureBoot\State\UEFISecureBootEnabled` (**its value**, never the key's existence) | `efivars/SecureBoot-8be4df61-…` byte 4 | omit |
| `bios_release_date` | `YYYY-MM-DD` | Profile | `Win32_BIOS.ReleaseDate` | `/sys/class/dmi/id/bios_date` | omit |
| `tpm_spec_version` | string, e.g. `2.0` | Profile | TPM2_GetCapability | same, or `/sys/class/tpm/tpm0/tpm_version_major` | omit |
| `tpm_vendor` | 4-char TCG id as read, e.g. `IFX` | Profile | TPM2_GetCapability `PT_MANUFACTURER` | same | omit |
| `tpm_vendor_name` | registry name, e.g. `Infineon` | Profile | TCG Vendor ID Registry lookup | same | omit |
| `tpm_firmware_version` | `major.minor.build.rev` | Profile | `PT_FIRMWARE_VERSION_1/2` | same | omit |
| `tpm_ek` | 64 lowercase hex | **Identity** (tier 2) | TPM2_ReadPublic via TBS | TPM2_ReadPublic via `/dev/tpmrm0` | omit |
| `tpm_ek_alg` | `rsa2048` \| `ecc-p256` | Profile | handle used | handle used | omit |
| `tpm_ek_cert_sha256` | 64 lowercase hex | Profile, **never identity** | NV index `0x01C00002` / `0x01C0000A` | same | omit |
| `disk.N.serial` | raw trimmed string | Profile (proposed) | `MSFT_PhysicalDisk.SerialNumber` | `/sys/block/*/device/serial` | omit (stage 2) |
| `monitor.N.edid_maker` | 3-letter PNP id | Profile | `WmiMonitorID` (Active) | `/sys/class/drm/*/edid` (connected) | omit (stage 2) |
| `monitor.N.edid_product` | int | Profile | `ProductCodeID` | EDID bytes 10–11 | |
| `monitor.N.edid_serial` | int | Profile (proposed) | EDID bytes 12–15 | EDID bytes 12–15 | |
| `monitor.N.edid_serial_text` | string | Profile (proposed) | `SerialNumberID` | descriptor tag `0xFF` | |
| `monitor.N.edid_year` | int | Profile | `YearOfManufacture` | EDID byte 17 + 1990 | |
| `gpu.N.uuid` | NVML string, `GPU-…` | Profile (proposed) | NVML, matched by PCI id | NVML (feature `nvidia`) | n/a |

Two design notes:

- **Monitors get their own `monitor.N` namespace.** They do not share `display.N`, because the EDID source and the display-mode source enumerate in different orders. A shared index would imply a pairing that was never observed.
- **Peripheral serials are proposed as profile-only.** The peripherals they identify (disks, monitors, GPUs) move between machines, which is the IronForensics point. The server decides whether any of them become identity.

## TPM endorsement key

The canonical form, as ruled server-side, is not advisory:

```
device.tpm_ek = lowercase hex SHA-256 of the DER SubjectPublicKeyInfo of the EK public key
```

1. Read persistent handle `0x81010001` (TCG template L-1, RSA-2048). If it is absent, read `0x81010002` (L-2, ECC P-256). If both are absent, **omit**. Never `CreatePrimary`: that needs endorsement-hierarchy authorisation, and a key created from another template is a different key.
2. Hash the DER SPKI, not the marshalled `TPM2B_PUBLIC`, the bare modulus, or raw `X||Y`.
   - RSA: `SEQUENCE { SEQUENCE { rsaEncryption, NULL }, BIT STRING { SEQUENCE { INTEGER n, INTEGER e } } }`. A TPM exponent of 0 means 65537.
   - ECC: `SEQUENCE { SEQUENCE { id-ecPublicKey, prime256v1 }, BIT STRING { 04 || X || Y } }`.
3. Send `tpm_ek_alg` with the handle that was used.

## Work, in order

### Phase 0: Contract and coordination (gates the release)

- [ ] Send the key table above to nervosys-v2 as a proposal, with types and identity or profile status
- [ ] Agree the integer fields (`edid_product`, `edid_serial`, `edid_year`) and add them to `is_integer_field`
- [ ] nervosys-v2 confirms whether each peripheral serial is profile-only or identity
- [ ] nervosys-v2 confirms the `tpm_ek` allowlist entry has **shipped**, not merely been agreed
- [ ] nervosys-v2 has a cloned-VM EK detector in place. A vTPM cloned without generalisation shares one EK, and 7.3.3 sends from VMs.
- [ ] Tell IronForensics the outcome. It may optionally map the posture keys in its `ironmon-snapshot` ingester; that is its call.

### Phase 1: Posture signals

- [ ] `firmware_type`: Windows `GetFirmwareType` (check that `Win32_System_SystemInformation` covers it), Linux `/sys/firmware/efi`
- [ ] `secure_boot`: Windows registry **value**, Linux efivar byte 4. Omit when unreadable; never default to false.
- [ ] `bios_release_date`: parse the CIM datetime (Windows) and `MM/DD/YYYY` (Linux) to ISO. Omit when unparseable.
- [ ] Unit tests for both date parsers and the efivar decode

### Phase 2: TPM (new `src/device_profile/tpm.rs`, or a submodule)

- [ ] Command marshalling: `TPM2_GetCapability(TPM_CAP_TPM_PROPERTIES, PT_FAMILY_INDICATOR, 13)` and `TPM2_ReadPublic(handle)`
- [ ] Response parsing with bounds checks on every length field. A malformed response yields omission, never a panic.
- [ ] Transport, Windows: TBS (`Tbsi_Context_Create`, `Tbsip_Submit_Command`). Add the `Win32_System_TpmBaseServices` feature to `windows`. Confirm a **non-elevated** process can run both commands.
- [ ] Transport, Linux: `/dev/tpmrm0` only (resource-managed). Not `/dev/tpm0`. Permission denied (root or the `tss` group is usually required) yields omission.
- [ ] Spec version fallback from sysfs when the device is not readable
- [ ] TCG Vendor ID Registry table (about 30 entries, factual) for `tpm_vendor_name`
- [ ] TPM2B_PUBLIC to SPKI DER for RSA-2048 and ECC P-256, then SHA-256 (`sha2` is already a dependency)
- [ ] Known-answer tests: SPKI bytes and hash against fixtures produced independently by OpenSSL for one RSA and one ECC key
- [ ] Parser tests from captured response bytes, including truncated and oversized lengths
- [ ] Stretch: `tpm_ek_cert_sha256` via `TPM2_NV_ReadPublic` plus chunked `TPM2_NV_Read`. Profile only. Drop it if it costs more than a day.

### Phase 3: Peripheral identity

- [ ] `disk.N.serial`: add `SerialNumber` to the existing `MSFT_PhysicalDisk` query. Linux reads `device/serial`, using the same `N` as the existing `disk.N.*` keys.
- [ ] EDID parser: header check, maker PNP decode, product, serial, year, descriptor `0xFF` text. Checksum verified; on a failed checksum, keep nothing.
- [ ] Monitors, Windows: `ROOT\WMI` `WmiMonitorID WHERE Active = TRUE` (u16 arrays to strings). Confirm it is readable without elevation.
- [ ] Monitors, Linux: `/sys/class/drm/card*-*/edid` where `status` is `connected`
- [ ] `gpu.N.uuid`: extend the Windows NVML matcher, add NVML on Linux under `nvidia`, and match by PCI id, never by enumeration order
- [ ] EDID parser tests from real EDID blobs, including a bad checksum and a missing serial

### Phase 4: Opt-in in the EU/EEA

- [ ] `region::in_eea()`. True if any of these hold:
  - the locale's region is an EEA country (EU 27 plus IS, LI, NO);
  - the IANA timezone is in an EEA zone list;
  - the Windows timezone key is in a list that errs inclusive (`W. Europe`, `Central Europe`, `Central European`, `Romance`, `GTB`, `E. Europe`, `FLE`, `GMT`).

  Over-inclusion only asks more people, which is the safe direction.
- [ ] Refactor `ConsentScope::default_granted` into a pure `default_granted_in(eea: bool)`, so the tests do not depend on the CI runner's region
- [ ] In the EEA, an unanswered Device Profile means **off**. A recorded answer wins in every region.
- [ ] `serve` in the EEA at an interactive terminal: bring back the one-time `[y/N]` question from c03c543 and record the answer. Non-interactive: send no profile, and print one line naming `ironmon privacy review`.
- [ ] `ironmon privacy status` says the profile is opt-in in this region and why
- [ ] Tests: the default per region, a recorded answer overriding the default, and the timezone and locale lists

### Phase 5: Disclosure

- [ ] Raise `DEVICE_NOTICE_VERSION` from 1 to 2, so existing users see the notice again for the wider scope
- [ ] Notice text: keep it categorical, and add the security chip (TPM) identifier and the serial numbers of disks, monitors and GPUs
- [ ] `ConsentScope::DeviceProfile` data points: itemise every new key for `ironmon privacy info`
- [ ] README Telemetry section, CLI.md Device Profile section, CHANGELOG `[Unreleased]`

### Phase 6: Verification

- [ ] `cargo clippy -j 1 --offline --lib --bins --tests -- -D warnings`, the full test suite, and `source_hygiene`
- [ ] Windows live: capture one push with the local sink and check every new key's presence, type and plausibility
- [ ] Windows TPM: check `tpm_ek` against an independent derivation of the same EK public key (PowerShell or NCrypt `PCP_EKPUB`, hashed as SPKI outside IronMonitor). Both runs must agree.
- [ ] Linux, WSL2: no TPM and no DRM monitors, so confirm clean omission and no zero substituted
- [ ] CI green on Windows, Linux and macOS
- [ ] With owner approval: one live push from a freshly started `serve`, with the time and install id sent to nervosys-v2 for the readback (matched tier, signal resolution, types)

### Phase 7: Release

- [ ] Phase 0 gates all checked
- [ ] Owner approves 7.4.0
- [ ] Release commit, tag, `.crate` and SHA256SUMS. The release notes say what the profile now covers, and that it is opt-in in the EU/EEA.

## Risks and open questions

- **Most Linux installs will not send `tpm_ek`.** `/dev/tpmrm0` usually needs root or the `tss` group. This is expected and is not worked around.
- **Windows non-admin TBS access** to `ReadPublic` on a persistent handle must be verified on hardware. If it is refused, omit, and do not ask for elevation.
- **Cloned VMs** share a vTPM EK. Detection belongs to the server, through the install-to-device link.
- **UK and Switzerland** are outside the EU/EEA rule as decided, though UK PECR has an equivalent consent rule. This needs an owner decision.
- **macOS** gets none of the TPM keys (there is no TPM). Disk serials and EDID come in a later stage, and none of it has run on a Mac.
- **Storage**, per nervosys-v2: the EBS volume and its snapshot are unencrypted, and there is no retention policy. That decision belongs to the owners, but it bears on adding stronger identifiers.
