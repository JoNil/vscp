extern crate byteorder;
extern crate i2cdev;

use byteorder::{LittleEndian, ReadBytesExt};
use i2cdev::core::I2CDevice;
use i2cdev::linux::LinuxI2CDevice;
use std::error::Error;
use std::io::Cursor;
use std::io::ErrorKind;
use std::net::UdpSocket;
use std::panic;
use std::process::Command;
use std::string::String;
use std::thread;
use std::time::{Duration, Instant};

const PCA9685_ADDRESS: u16 = 0x40;
const MODE1: u8 = 0x00;
const MODE2: u8 = 0x01;
const PRESCALE: u8 = 0xFE;
const LED0_ON_L: u8 = 0x06;
const LED0_ON_H: u8 = 0x07;
const LED0_OFF_L: u8 = 0x08;
const LED0_OFF_H: u8 = 0x09;
const ALL_LED_ON_L: u8 = 0xFA;
const ALL_LED_ON_H: u8 = 0xFB;
const ALL_LED_OFF_L: u8 = 0xFC;
const ALL_LED_OFF_H: u8 = 0xFD;

const SLEEP: u8 = 0x10;
const ALLCALL: u8 = 0x01;
const OUTDRV: u8 = 0x04;

#[derive(Clone, Copy, Default)]
struct Packet {
    forward_backward: f32,
    left_right: f32,
}

impl Packet {
    fn deserialize(cursor: &mut Cursor<&mut [u8]>) -> Result<Packet, &'static str> {
        let magic_u32 = cursor.read_u32::<LittleEndian>().unwrap_or(0);
        if magic_u32 == 0xAABBCCDD {
            Ok(Packet {
                forward_backward: cursor.read_f32::<LittleEndian>().unwrap_or(0.0),
                left_right: cursor.read_f32::<LittleEndian>().unwrap_or(0.0),
            })
        } else {
            Err("ERROR: INVALID MAGIC!")
        }
    }
}

struct Client {
    sock: UdpSocket,
    buf: [u8; 12],
}

impl Client {
    fn new(host: String) -> Client {
        let sock = UdpSocket::bind(host).expect("Could not bind hostname");
        sock.set_nonblocking(true)
            .expect("Failed to enter non-blocking mode");
        Client {
            sock: sock,
            buf: [0; 12],
        }
    }

    fn read_vscp(&mut self) -> (Packet, i32) {
        let mut packet_count = 0;
        let mut newdata = true;
        let mut new_packet = Default::default();
        while newdata {
            let result = self.sock.recv(&mut self.buf);
            match result {
                Ok(num_bytes) => {
                    let mut cursor = Cursor::new(&mut self.buf[..num_bytes]);
                    let packet_result = Packet::deserialize(&mut cursor);
                    match packet_result {
                        Ok(packet) => {
                            packet_count += 1;
                            new_packet = packet;
                        }
                        Err(str) => println!("Error: {}", str),
                    }
                }
                Err(ref err) if err.kind() != ErrorKind::WouldBlock => {
                    println!("Something went wrong: {}", err);
                }
                _ => {
                    newdata = false;
                }
            }
        }
        (new_packet, packet_count)
    }
}

fn set_pwm_freq(dev: &mut LinuxI2CDevice, freq_hz: u16) -> Result<(), Box<Error>> {
    let mut prescaleval = 25000000.0;
    prescaleval /= 4096.0;
    prescaleval /= freq_hz as f64;
    prescaleval -= 1.0;

    let prescale = (prescaleval + 0.5) as u8;
    let oldmode = dev.smbus_read_byte_data(MODE1)?;
    let newmode = (oldmode & 0x7F) | 0x10;

    dev.smbus_write_byte_data(MODE1, newmode)?;
    dev.smbus_write_byte_data(PRESCALE, prescale)?;
    dev.smbus_write_byte_data(MODE1, oldmode)?;

    thread::sleep(Duration::from_millis(50));

    dev.smbus_write_byte_data(MODE1, oldmode | 0x80)?;

    Ok(())
}

fn set_pwm(dev: &mut LinuxI2CDevice, channel: u8, on: u16, off: u16) -> Result<(), Box<Error>> {
    dev.smbus_write_byte_data(LED0_ON_L + 4 * channel, (on & 0xFF) as u8)?;
    dev.smbus_write_byte_data(LED0_ON_H + 4 * channel, (on >> 8) as u8)?;
    dev.smbus_write_byte_data(LED0_OFF_L + 4 * channel, (off & 0xFF) as u8)?;
    dev.smbus_write_byte_data(LED0_OFF_H + 4 * channel, (off >> 8) as u8)?;

    Ok(())
}

fn set_all_pwm(dev: &mut LinuxI2CDevice, on: u16, off: u16) -> Result<(), Box<Error>> {
    dev.smbus_write_byte_data(ALL_LED_ON_L, (on & 0xFF) as u8)?;
    dev.smbus_write_byte_data(ALL_LED_ON_H, (on >> 8) as u8)?;
    dev.smbus_write_byte_data(ALL_LED_OFF_L, (off & 0xFF) as u8)?;
    dev.smbus_write_byte_data(ALL_LED_OFF_H, (off >> 8) as u8)?;

    Ok(())
}

fn lte_monitor() -> Result<(), Box<Error>> {
    let mut connected;
    let mut connecting = false;
    let mut connecting_start_timer = Instant::now();

    loop {

        {
            let output = Command::new("ifconfig").arg("wwan0").output()?;
            let output_string = String::from_utf8_lossy(&output.stdout);

            let lte_ip = output_string
                .lines()
                .filter(|line| line.contains("inet "))
                .filter_map(|line| line.trim().split(" ").nth(1))
                .next();

            if let Some(ip) = lte_ip {
                if ip.starts_with("169.254") {
                    connected = false;
                } else {
                    connected = true;
                }
            } else {
                continue;
            }
        }

        // Start connection attempt
        if !connected && !connecting {

            connecting = true;
            connecting_start_timer = Instant::now();

            let stop_output = Command::new("qmi-network").arg("/dev/cdc-wdm0").arg("stop").output()?;
            let start_output = Command::new("qmi-network").arg("/dev/cdc-wdm0").arg("start").output()?;

            println!("{}", String::from_utf8_lossy(&stop_output.stdout));
            println!("{}", String::from_utf8_lossy(&start_output.stdout));
        }

        // Successfully connected
        if connected && connecting {
            connecting = false;
            
            let publish_output =  Command::new("/home/pi/vscp/publish_ip").output()?;

            println!("{}", String::from_utf8_lossy(&publish_output.stdout));
        }

        // Connection timed out
        if connecting && connecting_start_timer.elapsed().as_secs() > 60 {
            connecting = false;
        }

        thread::sleep(Duration::from_millis(1000));
    }
}

fn lte_monitor_thread() {
    let res = panic::catch_unwind(|| {
        let res = lte_monitor();
        if let Err(e) = res {
            println!("{}", e);
        }
    });

    if let Err(e) = res {
        println!("{:?}", e);
    }
}

fn main() -> Result<(), Box<Error>> {
    thread::spawn(lte_monitor_thread);

    let host = "0.0.0.0:50001";
    let mut client = Client::new(host.to_owned());

    let mut dev = LinuxI2CDevice::new("/dev/i2c-1", PCA9685_ADDRESS)?;

    set_all_pwm(&mut dev, 0, 0)?;
    dev.smbus_write_byte_data(MODE2, OUTDRV)?;
    dev.smbus_write_byte_data(MODE1, ALLCALL)?;

    thread::sleep(Duration::from_millis(50));

    let mut mode1 = dev.smbus_read_byte_data(MODE1)?;
    mode1 = mode1 & !SLEEP;
    dev.smbus_write_byte_data(MODE1, mode1)?;

    thread::sleep(Duration::from_millis(50));

    set_pwm_freq(&mut dev, 100)?;

    let mut last_packet_time = Instant::now();
    let mut packet = Default::default();

    loop {
        {
            let (new_packet, packet_count) = client.read_vscp();
            if packet_count > 0 {
                last_packet_time = Instant::now();
                packet = new_packet;
            }
        }

        if last_packet_time.elapsed().subsec_millis() > 200 {
            packet.forward_backward = 0.0;
            packet.left_right = 0.0;
        }

        set_pwm(&mut dev, 3, 0, (614.0 + 205.0 * packet.left_right) as u16).ok();

        thread::sleep(Duration::from_millis(10));

        set_pwm(
            &mut dev,
            1,
            0,
            (614.0 + 205.0 * packet.forward_backward) as u16,
        ).ok();

        thread::sleep(Duration::from_millis(10));
    }
}
