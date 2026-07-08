//! Форматирование строки лога без кучи и без `format!`.

/// Собрать `WORK=<work>mV REF=<reference>mV\r\n` в `buf`, вернуть длину.
pub fn line(buf: &mut [u8], work: u16, reference: u16) -> usize {
    let mut n = 0;
    let put = |buf: &mut [u8], n: &mut usize, s: &[u8]| {
        for &c in s {
            buf[*n] = c;
            *n += 1;
        }
    };
    put(buf, &mut n, b"WORK=");
    n += write_u16(&mut buf[n..], work);
    put(buf, &mut n, b"mV REF=");
    n += write_u16(&mut buf[n..], reference);
    put(buf, &mut n, b"mV\r\n");
    n
}

/// `val` как десятичное ASCII в `out`, без ведущих нулей. Возвращает число цифр.
fn write_u16(out: &mut [u8], val: u16) -> usize {
    let mut tmp = [0u8; 5];
    let mut i = 0;
    let mut v = val;
    loop {
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
        if v == 0 {
            break;
        }
    }
    for j in 0..i {
        out[j] = tmp[i - 1 - j];
    }
    i
}
