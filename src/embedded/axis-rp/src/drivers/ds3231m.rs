use bitfield::bitfield;
use defmt::error;
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

    async fn read(&mut self) -> Result<TimekeepingRegisters<[u8; 18]>, I2C::Error> {
        let buf = &mut [0u8; 18];
        self.i2c.read(self.address, buf).await.map(|_| TimekeepingRegisters(*buf))
    }

    pub async fn get_date_time(&mut self) -> Result<DateTime, I2C::Error> {
        self.read().await.map(|reg| reg.get_date_time())
    }
}

bitfield! {
    #[derive(Clone, Copy, Debug)]
    pub struct TimekeepingRegisters([u8]);
    u8;
    seconds_10, set_seconds_10:                 6 +0x00, 4 +0x00;
    seconds, set_seconds:                       3 +0x00, 0 +0x00;

    minutes_10, set_minutes_10:                 6 +0x01, 4 +0x01;
    minutes, set_minutes:                       3 +0x01, 4 +0x01;

    hours_20, set_hours_20:                     5 +0x02, 5 +0x02;
    hours_am_pm, set_hours_am_pm:               5 +0x02, 5 +0x02;
    hours_10, set_hours_10:                     4 +0x02, 4 +0x02;
    hours, set_hours:                           3 +0x02, 0 +0x02;

    day_day, set_day_day:                       2 +0x03, 0 +0x03;

    date_10, set_date_10:                       5 +0x04, 4 +0x04;
    date, set_date:                             3 +0x04, 0 +0x04;


    century, set_century:                       7 +0x05, 7 +0x05;
    month_10, set_month_10:                     4 +0x05, 4 +0x05;
    month, set_month:                           3 +0x05, 0 +0x05;

    year_10, set_year_10:                       7 +0x06, 4 +0x06;
    year, set_year:                             3 +0x06, 0 +0x06;

    // Alarm 1
    a1m1, set_a1m1:                             7 +0x07, 7 +0x07;
    alarm1_seconds_10, set_alarm1_seconds_10:   6 +0x07, 4 +0x07;
    alarm1_seconds, set_alarm1_seconds:         3 +0x07, 0 +0x07;

    a1m2, set_a1m2:                             7 +0x08, 7 +0x08;
    alarm1_minutes_10, set_alarm1_minutes_10:   6 +0x08, 4 +0x08;
    alarm1_minutes, set_alarm1_minutes:         3 +0x08, 0 +0x08;

    a1m3, set_a1m3:                             7 +0x09, 7 +0x09;
    alarm1_hours_12_24, set_alarm1_hours_12_24: 6 +0x09, 6 +0x09;
    alarm1_hours_20, set_alarm1_hours_20:       5 +0x09, 5 +0x09;
    alarm1_hours_am_pm, set_alarm1_hours_am_pm: 5 +0x09, 5 +0x09;
    alarm1_hours_10, set_alarm1_hours_10:       4 +0x09, 4 +0x09;
    alarm1_hours, set_alarm1_hours:             3 +0x09, 0 +0x09;

    a1m4, set_a1m4:                             7 +0x0a, 7 +0x0a;
    alarm1_dy_dt_flag, set_alarm1_dy_dt_flag:   6 +0x0a, 6 +0x0a;
    alarm1_date_10, set_alarm1_date_10:         5 +0x0a, 4 +0x0a;
    alarm1_day, set_alarm1_day:                 3 +0x0a, 0 +0x0a;
    alarm1_date, set_alarm1_date:               3 +0x0a, 0 +0x0a;

    a2m2, set_a2m2:                             7 +0x0b, 7 +0x0b;
    alarm2_minutes_10, set_alarm2_minutes_10:   6 +0x0b, 4 +0x0b;
    alarm2_minutes, set_alarm2_minutes:         3 +0x0b, 0 +0x0b;

    a2m3, set_a2m3:                             7 +0x0c, 7 +0x0c;
    alarm2_hours_12_24, set_alarm2_hours_12_24: 6 +0x0c, 6 +0x0c;
    alarm2_hours_20, set_alarm2_hours_20:       5 +0x0c, 5 +0x0c;
    alarm2_hours_am_pm, set_alarm2_hours_am_pm: 5 +0x0c, 5 +0x0c;
    alarm2_hours_10, set_alarm2_hours_10:       4 +0x0c, 4 +0x0c;
    alarm2_hours, set_alarm2_hours:             3 +0x0c, 0 +0x0c;

    a2m4, set_a2m4:                             7 +0x0d, 7 +0x0d;
    alarm2_dy_dt_flag, set_alarm2_dy_dt_flag:   6 +0x0d, 6 +0x0d;
    alarm2_date_10, set_alarm2_date_10:         5 +0x0d, 4 +0x0d;
    alarm2_day, set_alarm2_day:                 3 +0x0d, 0 +0x0d;
    alarm2_date, set_alarm2_date:               3 +0x0d, 0 +0x0d;

    // config registers
    esoc, set_esoc:                             7 +0x0e, 7 +0x0e;
    bbsqw, set_bbsqw:                           6 +0x0e, 6 +0x0e;
    conv, set_conv:                             5 +0x0e, 5 +0x0e;
    intcn, set_intcn:                           2 +0x0e, 2 +0x0e;
    a2ie, set_a2ie:                             1 +0x0e, 1 +0x0e;
    a1ie, set_a1ie:                             0 +0x0e, 0 +0x0e;

    // status registers
    osf, set_osf:                               7 +0x0f, 7 +0x0f;
    en32khz, set_en32khz:                       3 +0x0f, 3 +0x0f;
    bsy, _:                                     2 +0x0f, 2 +0x0f;
    a2f, _:                                     1 +0x0f, 1 +0x0f;
    a1f, _:                                     0 +0x0f, 0 +0x0f;

    // aging register
    aging_reg, set_aging_reg:                   7 +0x10, 0 +0x10;

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
            second: self.seconds() + (10 * self.seconds_10()),
            minute: self.minutes() + (10 * self.minutes_10()),
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

#[derive(Clone, Copy, Debug)]
pub struct DateTime {
    pub time: Time,
    pub date: u8,
    pub day: Day,
}

#[derive(Clone, Copy, Debug)]
pub struct Time {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub am_pm: bool
}

#[derive(Clone, Copy, Debug)]
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

