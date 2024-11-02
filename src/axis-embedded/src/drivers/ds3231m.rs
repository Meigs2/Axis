use bitfield::bitfield;
use defmt::{info, Format, Formatter};
use embedded_hal_async::i2c::I2c;

pub struct Ds3231m<I2C: I2c> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Ds3231m<I2C> {
    pub fn new(i2c: I2C, address: u8) -> Self {
        Self {
            i2c,
            address,
        }
    }

    async fn read(&mut self) -> Result<TimekeepingRegisters<[u8; 19]>, I2C::Error> {
        let buf = &mut [0u8; 19];
        self.i2c.read(self.address, buf).await.map(|_| {
            TimekeepingRegisters(*buf)
        })
    }

    pub async fn get_date_time(&mut self) -> Result<DateTime, I2C::Error> {
        self.read().await.map(|reg| reg.get_date_time())
    }
}

bitfield! {
    #[derive(Clone, Copy, Debug)]
    pub struct TimekeepingRegisters([u8]);
    u8;
    seconds_10, set_seconds_10:                 6, 4;
    seconds, set_seconds:                       3, 0;

    minutes_10, set_minutes_10:                 6 +0x01*8, 4 +0x01*8;
    minutes, set_minutes:                       3 +0x01*8, 0 +0x01*8;

    hours_20, set_hours_20:                     5 +0x02*8, 5 +0x02*8;
    hours_am_pm, set_hours_am_pm:               5 +0x02*8, 5 +0x02*8;
    hours_10, set_hours_10:                     4 +0x02*8, 4 +0x02*8;
    hours, set_hours:                           3 +0x02*8, 0 +0x02*8;

    day_day, set_day_day:                       2 +0x03*8, 0 +0x03*8;

    date_10, set_date_10:                       5 +0x04*8, 4 +0x04*8;
    date, set_date:                             3 +0x04*8, 0 +0x04*8;


    century, set_century:                       7 +0x05*8, 7 +0x05*8;
    month_10, set_month_10:                     4 +0x05*8, 4 +0x05*8;
    month, set_month:                           3 +0x05*8, 0 +0x05*8;

    year_10, set_year_10:                       7 +0x06*8, 4 +0x06*8;
    year, set_year:                             3 +0x06*8, 0 +0x06*8;

    // Alarm 1
    a1m1, set_a1m1:                             7 +0x07*8, 7 +0x07*8;
    alarm1_seconds_10, set_alarm1_seconds_10:   6 +0x07*8, 4 +0x07*8;
    alarm1_seconds, set_alarm1_seconds:         3 +0x07*8, 0 +0x07*8;

    a1m2, set_a1m2:                             7 +0x08*8, 7 +0x08*8;
    alarm1_minutes_10, set_alarm1_minutes_10:   6 +0x08*8, 4 +0x08*8;
    alarm1_minutes, set_alarm1_minutes:         3 +0x08*8, 0 +0x08*8;

    a1m3, set_a1m3:                             7 +0x09*8, 7 +0x09*8;
    alarm1_hours_12_24, set_alarm1_hours_12_24: 6 +0x09*8, 6 +0x09*8;
    alarm1_hours_20, set_alarm1_hours_20:       5 +0x09*8, 5 +0x09*8;
    alarm1_hours_am_pm, set_alarm1_hours_am_pm: 5 +0x09*8, 5 +0x09*8;
    alarm1_hours_10, set_alarm1_hours_10:       4 +0x09*8, 4 +0x09*8;
    alarm1_hours, set_alarm1_hours:             3 +0x09*8, 0 +0x09*8;

    a1m4, set_a1m4:                             7 +0x0a*8, 7 +0x0a*8;
    alarm1_dy_dt_flag, set_alarm1_dy_dt_flag:   6 +0x0a*8, 6 +0x0a*8;
    alarm1_date_10, set_alarm1_date_10:         5 +0x0a*8, 4 +0x0a*8;
    alarm1_day, set_alarm1_day:                 3 +0x0a*8, 0 +0x0a*8;
    alarm1_date, set_alarm1_date:               3 +0x0a*8, 0 +0x0a*8;

    a2m2, set_a2m2:                             7 +0x0b*8, 7 +0x0b*8;
    alarm2_minutes_10, set_alarm2_minutes_10:   6 +0x0b*8, 4 +0x0b*8;
    alarm2_minutes, set_alarm2_minutes:         3 +0x0b*8, 0 +0x0b*8;

    a2m3, set_a2m3:                             7 +0x0c*8, 7 +0x0c*8;
    alarm2_hours_12_24, set_alarm2_hours_12_24: 6 +0x0c*8, 6 +0x0c*8;
    alarm2_hours_20, set_alarm2_hours_20:       5 +0x0c*8, 5 +0x0c*8;
    alarm2_hours_am_pm, set_alarm2_hours_am_pm: 5 +0x0c*8, 5 +0x0c*8;
    alarm2_hours_10, set_alarm2_hours_10:       4 +0x0c*8, 4 +0x0c*8;
    alarm2_hours, set_alarm2_hours:             3 +0x0c*8, 0 +0x0c*8;

    a2m4, set_a2m4:                             7 +0x0d*8, 7 +0x0d*8;
    alarm2_dy_dt_flag, set_alarm2_dy_dt_flag:   6 +0x0d*8, 6 +0x0d*8;
    alarm2_date_10, set_alarm2_date_10:         5 +0x0d*8, 4 +0x0d*8;
    alarm2_day, set_alarm2_day:                 3 +0x0d*8, 0 +0x0d*8;
    alarm2_date, set_alarm2_date:               3 +0x0d*8, 0 +0x0d*8;

    // config registers
    esoc, set_esoc:                             7 +0x0e*8, 7 +0x0e*8;
    bbsqw, set_bbsqw:                           6 +0x0e*8, 6 +0x0e*8;
    conv, set_conv:                             5 +0x0e*8, 5 +0x0e*8;
    intcn, set_intcn:                           2 +0x0e*8, 2 +0x0e*8;
    a2ie, set_a2ie:                             1 +0x0e*8, 1 +0x0e*8;
    a1ie, set_a1ie:                             0 +0x0e*8, 0 +0x0e*8;

    // status registers
    osf, set_osf:                               7 +0x0f*8, 7 +0x0f*8;
    en32khz, set_en32khz:                       3 +0x0f*8, 3 +0x0f*8;
    bsy, _:                                     2 +0x0f*8, 2 +0x0f*8;
    a2f, _:                                     1 +0x0f*8, 1 +0x0f*8;
    a1f, _:                                     0 +0x0f*8, 0 +0x0f*8;

    // aging register
    aging_reg, set_aging_reg:                   7 +0x10*8, 0 +0x10*8;
}

impl<T: AsRef<[u8]>> TimekeepingRegisters<T> {
    pub fn get_date_time(&self) -> DateTime {
        DateTime {
            time: self.get_time(),
            date: self.get_date(),
            day: self.get_day(),
        }
    }

    pub fn get_time(&self) -> Time {
        let use_am_pm = self.hours_am_pm() > 0;
        Time {
            second: self.seconds() + (self.seconds_10() * 10),
            minute: self.minutes() + (self.minutes_10() * 10),
            am_pm: use_am_pm,
            hour: self.hours() + (10 * self.hours_10()) +
                (match use_am_pm {
                    true => 0,
                    false => 20 * self.hours_20()
            })
        }
    }

    pub fn get_day(&self) -> Day {
        match self.day_day() {
            1 => Day::Monday,
            2 => Day::Tuesday,
            3 => Day::Wednesday,
            4 => Day::Thursday,
            5 => Day::Friday,
            6 => Day::Saturday,
            7 => Day::Sunday,
            _ => Day::None,
        }
    }

    pub fn get_date(&self) -> u8 {
        self.date() + (10 * self.date_10())
    }
}

#[derive(Clone, Copy, Debug, Format)]
pub struct DateTime {
    pub time: Time,
    pub date: u8,
    pub day: Day,
}

#[derive(Clone, Copy, Debug, Format)]
pub struct Time {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub am_pm: bool
}

#[derive(Clone, Copy, Debug, Format)]
#[repr(u8)]
pub enum Day {
    None = 0,
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
    Sunday = 7,
}

