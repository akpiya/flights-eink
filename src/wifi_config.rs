use esp_bootloader_esp_idf::partitions::{self, FlashStorage, PARTITION_TABLE_MAX_LEN};
use heapless::String;

pub struct WifiConfig {
    pub ssid: String<32>,
    pub password: String<64>,
}

/// Reads the WiFi details `tools/flash-wifi.sh` wrote to the `config` partition. They
/// live outside the firmware so the publicly published images contain no secrets.
pub fn read(flash: &mut FlashStorage<'_>) -> Option<WifiConfig> {
    let mut table = [0; PARTITION_TABLE_MAX_LEN];
    let partitions = partitions::read_partition_table(flash, &mut table).ok()?;
    let config = partitions.iter().find(|entry| entry.label_as_str() == "config")?;

    // "SSID\npassword\n": at most 32 + 64 bytes plus two newlines. Unwritten flash
    // reads as 0xFF, which isn't UTF-8, so a missing config fails to parse.
    let mut bytes = [0; 98];
    config.as_flash_region(flash).read(0, &mut bytes).ok()?;
    let mut lines = bytes.split(|&byte| byte == b'\n');
    let ssid = core::str::from_utf8(lines.next()?).ok()?;
    let password = core::str::from_utf8(lines.next()?).ok()?;
    Some(WifiConfig {
        ssid: ssid.try_into().ok()?,
        password: password.try_into().ok()?,
    })
}
