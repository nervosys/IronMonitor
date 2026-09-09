// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! SATA SMART attributes on Windows, without elevation.
//!
//! `IOCTL_STORAGE_PREDICT_FAILURE` returns a `STORAGE_PREDICT_FAILURE`: the
//! drive's own failure prediction, plus 512 vendor-specific bytes which, for an
//! ATA device, are verbatim the SMART READ DATA structure that
//! [`super::ata_smart`] parses. The storage driver issues the ATA command on the
//! caller's behalf, so the caller never needs to.
//!
//! # Why not `IOCTL_ATA_PASS_THROUGH`
//!
//! Because it cannot be done unelevated, which is the whole point of the exercise.
//! `IOCTL_ATA_PASS_THROUGH` is declared `CTL_CODE(IOCTL_SCSI_BASE, 0x040b,
//! METHOD_BUFFERED, FILE_READ_ACCESS | FILE_WRITE_ACCESS)`, so the I/O manager
//! checks the handle's access mask before the driver is ever reached — and a
//! handle on `\\.\PhysicalDriveN` with read and write access requires
//! Administrator. Issued on the zero-access handle that makes the NVMe path work,
//! it returns `ERROR_ACCESS_DENIED` (0x80070005). Measured on all four physical
//! drives of the development machine, unelevated.
//!
//! `IOCTL_STORAGE_PREDICT_FAILURE` is `CTL_CODE(IOCTL_STORAGE_BASE, 0x0440,
//! METHOD_BUFFERED, FILE_ANY_ACCESS)`. On the same zero-access handle it reaches
//! the driver, which is what makes this route viable at all.
//!
//! # What this cannot get
//!
//! Failure thresholds. Those come from SMART READ THRESHOLDS, a separate ATA
//! command with no `IOCTL_STORAGE_*` equivalent, so every attribute here is
//! reported with a threshold of zero — which is also ATA's own encoding for "no
//! threshold", the value that never trips.
//!
//! # Verification status
//!
//! **The ioctl path has not been run against a SATA drive.** The development
//! machine has three NVMe drives and no ATA device; on all of them this returns
//! `Error::NotSupported`, derived from the `ERROR_INVALID_FUNCTION` the NVMe
//! driver answers with — which does confirm the control code passes the access
//! check on a zero-access handle, and that the not-an-ATA-device case is handled,
//! but says nothing about the parse. The 512-byte structure is covered by
//! [`super::ata_smart`]'s tests against synthetic buffers. Anyone with a SATA SSD
//! or HDD should compare `ironmon disk smart` against `smartctl -A` once; that is
//! the check this has not had.

use super::ata_smart::{AtaSmartData, STRUCTURE_LEN};
use super::traits::Error;
use super::windows_device::Device;
use windows::Win32::Foundation::{ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER};
use windows::Win32::System::Ioctl::{IOCTL_STORAGE_PREDICT_FAILURE, STORAGE_PREDICT_FAILURE};
use windows::Win32::System::IO::DeviceIoControl;

/// What the drive answered.
pub(crate) struct AtaHealth {
    /// The drive's own SMART verdict: true when it predicts its own failure.
    ///
    /// This is the `PredictFailure` field, which the storage stack fills in for
    /// devices that have no parsable attribute structure as well, so it is
    /// meaningful even when `smart` is `None`.
    pub predict_failure: bool,
    /// The parsed attribute structure, when the vendor-specific bytes held one.
    ///
    /// `None` for a device whose driver reports a prediction but returns nothing
    /// usable in the 512-byte area — a USB bridge that does not tunnel SMART, for
    /// instance. The prediction is still a reading in that case.
    pub smart: Option<AtaSmartData>,
}

/// Read the drive's failure prediction and, where available, its SMART attributes.
///
/// Returns [`Error::NotSupported`] when the device does not implement the control
/// code, which is how NVMe drives and most USB bridges answer.
pub(crate) fn query(index: u32) -> Result<AtaHealth, Error> {
    let device = Device::open(index)?;

    let mut out = STORAGE_PREDICT_FAILURE::default();
    let mut returned = 0u32;

    unsafe {
        DeviceIoControl(
            device.0,
            IOCTL_STORAGE_PREDICT_FAILURE,
            None,
            0,
            Some(&mut out as *mut _ as *mut _),
            std::mem::size_of::<STORAGE_PREDICT_FAILURE>() as u32,
            Some(&mut returned),
            None,
        )
    }
    .map_err(|e| {
        // A device with no ATA SMART behind it declines the control code rather
        // than failing to execute it. NVMe drives answer ERROR_INVALID_FUNCTION;
        // some bridges answer ERROR_INVALID_PARAMETER. Both mean "not me", and
        // reporting either as a query failure would put an error in the caller's
        // log for every NVMe drive on the system on every poll.
        let code = e.code();
        if code == ERROR_INVALID_FUNCTION.to_hresult()
            || code == ERROR_INVALID_PARAMETER.to_hresult()
        {
            Error::NotSupported
        } else {
            Error::QueryFailed(format!("predict failure query on drive {index}: {e}"))
        }
    })?;

    // A short reply means the driver wrote the prediction but not the structure.
    // Parsing what it did not write would read whatever `default()` left there,
    // which the checksum would usually catch — but only usually, and this is
    // cheaper than relying on that.
    let smart = if returned as usize >= std::mem::size_of::<STORAGE_PREDICT_FAILURE>() {
        AtaSmartData::parse(&out.VendorSpecific[..STRUCTURE_LEN])
    } else {
        None
    };

    Ok(AtaHealth {
        // Non-zero means the drive predicts failure. The field is a DWORD rather
        // than a boolean because the ATA status byte it derives from is one.
        predict_failure: out.PredictFailure != 0,
        smart,
    })
}
