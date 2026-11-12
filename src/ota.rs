use defmt::info;
use embedded_io_async::Read;
use esp_bootloader_esp_idf::{
    ota::OtaImageState,
    ota_updater::OtaUpdater,
    partitions::{FlashStorage, PARTITION_TABLE_MAX_LEN},
};
use heapless::String;
use reqwless::request::Method;
use sha2::{Digest, Sha256};

use crate::https::Https;

/// The commit this firmware was built from. GitHub Actions sets GITHUB_SHA; local
/// builds are "dev" and never update themselves, so a USB-flashed build isn't
/// immediately replaced by the published one.
pub const VERSION: &str = match option_env!("GITHUB_SHA") {
    Some(sha) => sha,
    None => "dev",
};

// Published by .github/workflows/release.yml. GitHub Pages rather than Releases
// because release downloads redirect to another host, and reqwless doesn't follow
// redirects.
const MANIFEST_URL: &str = "https://akpiya.github.io/flights-eink/version.txt";
const FIRMWARE_URL: &str = "https://akpiya.github.io/flights-eink/firmware.bin";

const CHUNK_BYTES: usize = 4096;

#[derive(Debug, defmt::Format)]
pub enum UpdateError {
    Http(reqwless::Error),
    Status(u16),
    Manifest,
    Flash,
    Size,
    Checksum,
}

/// Rolls back an update that failed its first boot.
///
/// espflash's bootloader is built without rollback support, so it never marks a
/// failed image itself, but it does skip a slot marked Invalid. A new image is
/// therefore marked PendingVerify on its first boot; still finding PendingVerify on
/// the next boot means that boot crashed before reaching `mark_healthy`.
pub fn check_previous_boot(flash: &mut FlashStorage<'_>) {
    let mut table = [0; PARTITION_TABLE_MAX_LEN];
    let Ok(mut ota) = OtaUpdater::new(flash, &mut table) else {
        return;
    };
    match ota.current_ota_state() {
        Ok(OtaImageState::New) => {
            let _ = ota.set_current_ota_state(OtaImageState::PendingVerify);
        }
        Ok(OtaImageState::PendingVerify) => {
            info!("Update failed its first boot; rolling back");
            let _ = ota.set_current_ota_state(OtaImageState::Invalid);
            esp_hal::system::software_reset();
        }
        _ => {}
    }
}

/// Confirms a freshly installed update works, so it's kept.
pub fn mark_healthy(flash: &mut FlashStorage<'_>) {
    let mut table = [0; PARTITION_TABLE_MAX_LEN];
    let Ok(mut ota) = OtaUpdater::new(flash, &mut table) else {
        return;
    };
    if ota.current_ota_state() == Ok(OtaImageState::PendingVerify) {
        let _ = ota.set_current_ota_state(OtaImageState::Valid);
        info!("Update confirmed healthy");
    }
}

/// Installs the published firmware if it's from a different commit, then restarts
/// into it. Returns normally when there's nothing to do.
pub async fn update(https: &mut Https<'_>, flash: &mut FlashStorage<'_>) -> Result<(), UpdateError> {
    if VERSION == "dev" {
        return Ok(());
    }
    let (version, size, checksum) = fetch_manifest(https).await?;
    if version == VERSION {
        return Ok(());
    }
    info!("Updating from {} to {}", VERSION, version.as_str());

    let (mut client, buffer) = https.client();
    let (headers, chunk) = buffer.split_at_mut(buffer.len() - CHUNK_BYTES);
    let mut request = client
        .request(Method::GET, FIRMWARE_URL)
        .await
        .map_err(UpdateError::Http)?;
    let response = request.send(headers).await.map_err(UpdateError::Http)?;
    if !response.status.is_successful() {
        return Err(UpdateError::Status(response.status.0));
    }
    let mut body = response.body().reader();

    let mut table = [0; PARTITION_TABLE_MAX_LEN];
    let mut ota = OtaUpdater::new(flash, &mut table).map_err(|_| UpdateError::Flash)?;
    {
        let (mut slot, _) = ota.next_partition().map_err(|_| UpdateError::Flash)?;
        if size > slot.capacity() {
            return Err(UpdateError::Size);
        }
        // The image is too big for RAM, so it streams into the spare slot a chunk at
        // a time. A partial download is harmless: the slot isn't activated until the
        // whole image has been checked.
        let mut hasher = Sha256::new();
        let mut written = 0;
        loop {
            let mut filled = 0;
            while filled < chunk.len() {
                let read = body.read(&mut chunk[filled..]).await.map_err(UpdateError::Http)?;
                if read == 0 {
                    break;
                }
                filled += read;
            }
            hasher.update(&chunk[..filled]);
            slot.write(written as u32, &chunk[..filled])
                .map_err(|_| UpdateError::Flash)?;
            written += filled;
            if filled < chunk.len() {
                break;
            }
        }
        if written != size {
            return Err(UpdateError::Size);
        }
        if hasher.finalize()[..] != checksum {
            return Err(UpdateError::Checksum);
        }
    }

    ota.activate_next_partition().map_err(|_| UpdateError::Flash)?;
    ota.set_current_ota_state(OtaImageState::New)
        .map_err(|_| UpdateError::Flash)?;
    info!("Update installed; restarting");
    esp_hal::system::software_reset();
}

/// version.txt is "<commit SHA> <image size> <image SHA-256 in hex>".
async fn fetch_manifest(https: &mut Https<'_>) -> Result<(String<40>, usize, [u8; 32]), UpdateError> {
    let (mut client, buffer) = https.client();
    let mut request = client
        .request(Method::GET, MANIFEST_URL)
        .await
        .map_err(UpdateError::Http)?;
    let response = request.send(buffer).await.map_err(UpdateError::Http)?;
    if !response.status.is_successful() {
        return Err(UpdateError::Status(response.status.0));
    }
    let body = response.body().read_to_end().await.map_err(UpdateError::Http)?;

    let text = core::str::from_utf8(body).map_err(|_| UpdateError::Manifest)?;
    let mut fields = text.split_whitespace();
    let version = fields.next().and_then(|sha| sha.try_into().ok());
    let size = fields.next().and_then(|size| size.parse().ok());
    let checksum = fields.next().and_then(parse_hex);
    match (version, size, checksum) {
        (Some(version), Some(size), Some(checksum)) => Ok((version, size, checksum)),
        _ => Err(UpdateError::Manifest),
    }
}

fn parse_hex(hex: &str) -> Option<[u8; 32]> {
    let mut bytes = [0; 32];
    if hex.len() != 64 {
        return None;
    }
    for (byte, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(core::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(bytes)
}
