// Misc util functions.

use time;

pub fn timespec_sub(a: &time::Timespec, b: &time::Timespec) -> time::Timespec {
    let mut res = *a;
    res.nsec -= b.nsec;
    if res.nsec < 0 {
        res.sec -= 1;
        res.nsec += 1000_000_000;
    }
    res.sec -= b.sec;
    res
}
