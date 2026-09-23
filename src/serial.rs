use core::arch::asm;
use core::fmt;

pub const COM1: u16 = 0x3F8;
pub const COM2: u16 = 0x2F8;

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let ret: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") ret, options(nomem, nostack, preserves_flags));
    }
    ret
}

pub struct SerialPort {
    pub base: u16,
}

impl SerialPort {
    pub const fn new(base: u16) -> Self {
        Self { base }
    }

    pub fn init(&self) {
        unsafe {
            outb(self.base + 1, 0x00); 
            outb(self.base + 3, 0x80); 
            outb(self.base + 0, 0x03); 
            outb(self.base + 1, 0x00);
            outb(self.base + 3, 0x03); 
            outb(self.base + 2, 0xC7); 
            outb(self.base + 1, 0x01); // تفعيل مقاطعات استقبال البيانات Data Available
            outb(self.base + 4, 0x0B); 
        }
    }

    pub fn read_status(&self) -> u8 {
        unsafe { inb(self.base + 5) }
    }

    pub fn read_byte(&self) -> u8 {
        unsafe { inb(self.base) }
    }

    fn is_transmit_empty(&self) -> bool {
        unsafe { inb(self.base + 5) & 0x20 != 0 }
    }

    pub fn send_byte(&self, byte: u8) {
        while !self.is_transmit_empty() {
            core::hint::spin_loop();
        }
        unsafe { outb(self.base, byte); }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' { self.send_byte(b'\r'); }
            self.send_byte(byte);
        }
        Ok(())
    }
}

pub static SERIAL1: SerialPort = SerialPort::new(COM1);
pub static SERIAL2: SerialPort = SerialPort::new(COM2);

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let mut serial = SerialPort::new(COM1);
    serial.write_fmt(args).ok();
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => { $crate::serial::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => { $crate::serial::_print(format_args!("{}\n", format_args!($($arg)*))) };
}
