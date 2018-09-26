extern crate i2cdev;
extern crate vscp_udp;

use i2cdev::core::I2CDevice;
use i2cdev::linux::LinuxI2CDevice;
use std::error::Error;
use std::thread;
use std::time::Duration;

const PCA9685_ADDRESS: u16 = 0x40;
const MODE1: u8            = 0x00;
const MODE2: u8            = 0x01;
const PRESCALE: u8         = 0xFE;
const LED0_ON_L: u8       = 0x06;
const LED0_ON_H: u8       = 0x07;
const LED0_OFF_L: u8       = 0x08;
const LED0_OFF_H: u8       = 0x09;
const ALL_LED_ON_L: u8     = 0xFA;
const ALL_LED_ON_H: u8     = 0xFB;
const ALL_LED_OFF_L: u8    = 0xFC;
const ALL_LED_OFF_H: u8    = 0xFD;

const SLEEP: u8            = 0x10;
const ALLCALL: u8          = 0x01;
const OUTDRV: u8           = 0x04;

fn set_pwm_freq(dev: &mut LinuxI2CDevice, freq_hz: u16) -> Result<(), Box<Error>> {
    let mut prescaleval = 25000000.0;
    prescaleval /= 4096.0;
    prescaleval /= freq_hz as f64;
    prescaleval -= 1.0;

    println!("{}", prescaleval);

    let prescale = (prescaleval + 0.5) as u8;
    let oldmode = dev.smbus_read_byte_data(MODE1)?;
    let newmode = (oldmode & 0x7F) | 0x10;

    println!("{:?}", prescale);
    println!("{:?}", oldmode);
    println!("{:?}", newmode);

    dev.smbus_write_byte_data(MODE1, newmode)?;
    dev.smbus_write_byte_data(PRESCALE, prescale)?;
    dev.smbus_write_byte_data(MODE1, oldmode)?;
    
    thread::sleep(Duration::from_millis(50));

    dev.smbus_write_byte_data(MODE1, oldmode | 0x80)?;

    Ok(())
}

fn set_pwm(dev: &mut LinuxI2CDevice, channel: u8, on: u16, off: u16) -> Result<(), Box<Error>> {
    
    dev.smbus_write_byte_data(LED0_ON_L+4*channel,  (on & 0xFF) as u8)?;
    dev.smbus_write_byte_data(LED0_ON_H+4*channel,  (on >> 8) as u8)?;
    dev.smbus_write_byte_data(LED0_OFF_L+4*channel, (off & 0xFF) as u8)?;
    dev.smbus_write_byte_data(LED0_OFF_H+4*channel, (off >> 8) as u8)?;

    Ok(())
}

fn set_all_pwm(dev: &mut LinuxI2CDevice, on: u16, off: u16) -> Result<(), Box<Error>> {
    
    dev.smbus_write_byte_data(ALL_LED_ON_L,  (on & 0xFF) as u8)?;
    dev.smbus_write_byte_data(ALL_LED_ON_H,  (on >> 8) as u8)?;
    dev.smbus_write_byte_data(ALL_LED_OFF_L, (off & 0xFF) as u8)?;
    dev.smbus_write_byte_data(ALL_LED_OFF_H, (off >> 8) as u8)?;

    Ok(())
}

fn main() -> Result<(), Box<Error>> {

    let host = "10.0.0.44:50001";
    let mut client = vscp_udp::Client::new(host.to_owned());
    let mut buf : [u8; 12] = [0; 12];

    let mut dev = LinuxI2CDevice::new("/dev/i2c-1", PCA9685_ADDRESS)?;

    set_all_pwm(&mut dev, 0, 0)?;
    dev.smbus_write_byte_data(MODE2, OUTDRV)?;
    dev.smbus_write_byte_data(MODE1, ALLCALL)?;
    
    thread::sleep(Duration::from_millis(50));

    let mut mode1 = dev.smbus_read_byte_data(MODE1)?;
    mode1 = mode1 & !SLEEP;
    dev.smbus_write_byte_data(MODE1, mode1)?;

    thread::sleep(Duration::from_millis(50));

    set_pwm_freq(&mut dev, 60)?;

    loop {



        let packet = client.read_vscp(&mut buf);
        println!("Values sent are:  forward_backward:{:?}, left_right:{:?}", packet.forward_backward, packet.left_right);

        set_pwm(&mut dev, 3, 0, (370.0 + 100.0 * packet.left_right) as u16)?;

        thread::sleep(Duration::from_millis(10));

        let forward = if packet.forward_backward > 0.0 {
            packet.forward_backward*0.5
        } else {
            0.0
        };

        set_pwm(&mut dev, 1, 0, (370.0 + 100.0 * forward) as u16)?;

        thread::sleep(Duration::from_millis(10));
    }
}
