use core::arch::asm;

const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

#[derive(Copy, Clone, Debug)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags)); }
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe { asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags)); }
    val
}

unsafe fn read_register(reg: u8) -> u8 {
    unsafe {
        outb(CMOS_ADDR, reg);
        inb(CMOS_DATA)
    }
}

fn bcd_to_bin(val: u8) -> u8 {
    ((val >> 4) * 10) + (val & 0x0F)
}

pub fn get_riyadh_time() -> DateTime {
    unsafe {
        while (read_register(0x0A) & 0x80) != 0 {
            core::hint::spin_loop();
        }

        let mut sec = read_register(0x00);
        let mut min = read_register(0x02);
        let mut hour = read_register(0x04);
        let mut day = read_register(0x07);
        let mut mon = read_register(0x08);
        let mut year = read_register(0x09) as u16;

        let reg_b = read_register(0x0B);

        if (reg_b & 0x04) == 0 {
            sec = bcd_to_bin(sec);
            min = bcd_to_bin(min);
            hour = bcd_to_bin(hour);
            day = bcd_to_bin(day);
            mon = bcd_to_bin(mon);
            year = bcd_to_bin(year as u8) as u16;
        }

        if (reg_b & 0x02) == 0 && (hour & 0x80) != 0 {
            hour = ((hour & 0x7F) + 12) % 24;
        }

        year += 2000;

        let mut riyadh_hour = hour + 3;
        let mut riyadh_day = day;

        if riyadh_hour >= 24 {
            riyadh_hour -= 24;
            riyadh_day += 1;
            if riyadh_day > 30 {
                riyadh_day = 1;
                mon = (mon % 12) + 1;
            }
        }

        DateTime {
            year,
            month: mon,
            day: riyadh_day,
            hour: riyadh_hour,
            minute: min,
            second: sec,
        }
    }
}
