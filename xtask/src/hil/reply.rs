//! What `cargo xtask hil` reads: a line of the board's console reply, such as `result:`, and the last report a stock
//! iperf prints with `-y C`.

/// One end's final report.
#[derive(Debug, PartialEq)]
pub struct Counts {
    /// The length of the report, from its start to its end, in hundredths of a second.
    pub centiseconds: u64,
    pub bytes: u64,
    /// The lost and the out-of-order datagrams of a UDP report.
    pub udp: Option<(u64, u64)>,
}

/// Reads the console line `<key>: <a>-<b> s <bytes> bytes <rate> Mbit/s`, and its ` lost <n>/<n> ooo <n>` when it
/// has them, from what `mcu-run console` printed.
pub fn board(printed: &str, key: &str) -> Result<Counts, String> {
    let prefix = format!("{key}: ");
    let line = printed
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .ok_or_else(|| format!("no {key} line in:\n{printed}"))?;
    let unreadable = || format!("an unreadable {key} line: {line}");
    let words: Vec<&str> = line.split_whitespace().collect();
    let (interval, bytes) = match words.as_slice() {
        [interval, "s", bytes, "bytes", _, "Mbit/s", ..] => (*interval, *bytes),
        _ => return Err(unreadable()),
    };
    let udp = match words.get(6..10) {
        Some(["lost", counts, "ooo", out_of_order]) => {
            let (lost, _) = counts.split_once('/').ok_or_else(unreadable)?;
            Some((number(lost)?, number(out_of_order)?))
        }
        _ => None,
    };
    Ok(Counts {
        centiseconds: length(interval).ok_or_else(unreadable)?,
        bytes: number(bytes)?,
        udp,
    })
}

/// Reads the last report line that a stock iperf printed with `-y C`. A TCP line has 9 fields: the interval in the
/// seventh and the bytes in the eighth. A UDP line adds the jitter, the lost datagrams, the total, the percent lost
/// and the out-of-order datagrams.
pub fn stock(printed: &str) -> Result<Counts, String> {
    let line = printed
        .lines()
        .rfind(|line| matches!(line.split(',').count(), 9 | 14))
        .ok_or_else(|| format!("no report in:\n{printed}"))?;
    let fields: Vec<&str> = line.split(',').collect();
    let field = |at: usize| fields.get(at).copied().unwrap_or_default();
    let udp = if fields.len() == 14 {
        Some((number(field(10))?, number(field(13))?))
    } else {
        None
    };
    Ok(Counts {
        centiseconds: length(field(6)).ok_or_else(|| format!("an unreadable report: {line}"))?,
        bytes: number(field(7))?,
        udp,
    })
}

fn number(text: &str) -> Result<u64, String> {
    text.parse()
        .map_err(|_| format!("{text} is not a whole number"))
}

/// The length of an interval such as `0.00-3.80` or `0.0-5.0`, in hundredths of a second.
fn length(interval: &str) -> Option<u64> {
    let (start, end) = interval.split_once('-')?;
    centiseconds(end)?.checked_sub(centiseconds(start)?)
}

/// Seconds with up to two decimals, such as `3.8` or `3.80`, in hundredths.
fn centiseconds(seconds: &str) -> Option<u64> {
    let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    let hundredths = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<u64>().ok()?.checked_mul(10)?,
        2 => fraction.parse().ok()?,
        _ => return None,
    };
    whole
        .parse::<u64>()
        .ok()?
        .checked_mul(100)?
        .checked_add(hundredths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_line_gives_its_length_and_bytes() {
        let printed = "result: 0.00-3.80 s 44727100 bytes 94.00 Mbit/s\nok\n";
        assert_eq!(
            board(printed, "result"),
            Ok(Counts {
                centiseconds: 380,
                bytes: 44_727_100,
                udp: None
            })
        );
    }

    #[test]
    fn a_board_udp_line_gives_its_lost_and_out_of_order_counts() {
        let printed = "result: 0.25-5.01 s 59380000 bytes 99.79 Mbit/s lost 212/40392 ooo 3 jitter 0.012 ms\n\
                       server: 0.00-5.00 s 59382940 bytes 95.01 Mbit/s lost 0/40393 ooo 0 jitter 0.010 ms\nok\n";
        assert_eq!(
            board(printed, "server"),
            Ok(Counts {
                centiseconds: 500,
                bytes: 59_382_940,
                udp: Some((0, 0))
            })
        );
        assert_eq!(
            board(printed, "result"),
            Ok(Counts {
                centiseconds: 476,
                bytes: 59_380_000,
                udp: Some((212, 3))
            })
        );
    }

    #[test]
    fn a_reply_without_the_line_is_an_error() {
        assert!(board("server: none\nok\n", "result").is_err());
        assert!(board("result: 1.00 s\nok\n", "result").is_err());
    }

    // The names avoid `stock_`, which `.cargo/mutants.toml` skips as the tests that run a stock binary.
    #[test]
    fn a_csv_report_is_the_last_line_of_9_or_14_fields() {
        let tcp =
            "20261018120000,192.168.10.1,5001,192.168.10.2,49152,1,0.0-5.0,59506688,95210700\n";
        assert_eq!(
            stock(tcp),
            Ok(Counts {
                centiseconds: 500,
                bytes: 59_506_688,
                udp: None
            })
        );
        let udp = "20261018120000,192.168.10.2,5001,192.168.10.1,49153,1,0.0-5.0,59380000,95008000\n\
                   20261018120005,192.168.10.2,5001,192.168.10.1,49153,1,0.0-4.9,59382940,95000000,0.010,7,40393,0.017,2\n";
        assert_eq!(
            stock(udp),
            Ok(Counts {
                centiseconds: 490,
                bytes: 59_382_940,
                udp: Some((7, 2))
            })
        );
    }

    #[test]
    fn output_without_a_csv_report_is_an_error() {
        assert!(stock("iperf: connect failed: Connection refused\n").is_err());
    }

    #[test]
    fn an_interval_in_whole_seconds_reads() {
        let tcp = "20261018120000,192.168.10.1,5001,192.168.10.2,49152,1,0-5,59506688,95210700\n";
        assert_eq!(stock(tcp).map(|counts| counts.centiseconds), Ok(500));
    }
}
