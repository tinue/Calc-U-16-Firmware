//! Boot ROM IAP entry (0x1FFF1FF1) emulation.

/// IAP command codes (UM10398 chapter "Flash memory programming firmware").
pub const PREPARE: u32 = 50;
pub const COPY_RAM_TO_FLASH: u32 = 51;
pub const ERASE: u32 = 52;
pub const BLANK_CHECK: u32 = 53;
pub const READ_PART_ID: u32 = 54;
pub const READ_BOOT_VERSION: u32 = 55;
pub const COMPARE: u32 = 56;
pub const REINVOKE_ISP: u32 = 57;
pub const READ_UID: u32 = 58;

pub const CMD_SUCCESS: u32 = 0;
pub const INVALID_COMMAND: u32 = 1;

pub const PART_ID: u32 = 0x1A40_902B;
pub const UID: [u32; 4] = [0x0D16_0001, 0x5357_4953, 0x534D_4943, 0x524F_5321];

pub fn name(cmd: u32) -> &'static str {
    match cmd {
        PREPARE => "prepare sectors",
        COPY_RAM_TO_FLASH => "copy RAM to flash",
        ERASE => "erase sectors",
        BLANK_CHECK => "blank check",
        READ_PART_ID => "read part ID",
        READ_BOOT_VERSION => "read boot code version",
        COMPARE => "compare",
        REINVOKE_ISP => "reinvoke ISP",
        READ_UID => "read UID",
        _ => "unknown",
    }
}
