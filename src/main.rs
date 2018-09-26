extern crate i2cdev;

use std::thread;
use std::time::Duration;

use i2cdev::core::*;
use i2cdev::linux::LinuxI2CDevice;

const PCA_9685_ADDR: u16 = 0x40;

fn main() {
    let mut dev = (LinuxI2CDevice::new("/dev/i2c-1", PCA_9685_ADDR)).expect("1");

    (dev.smbus_write_byte_data(0xF0, 0x55)).expect("2");
    (dev.smbus_write_byte_data(0xFB, 0x00)).expect("3");

    thread::sleep(Duration::from_millis(100));
}
