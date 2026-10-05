//! The layout of a sample file.
//!
//! A sample file is a list of records in the order the relay saw them. Each record is one byte for the direction,
//! a 32-bit big-endian length, and that many bytes. The direction is 0 for bytes from the client to the server
//! and 1 for bytes from the server to the client. A UDP record is one datagram. A TCP record is one read from the
//! socket, so its length means nothing.

/// Which way a record's bytes went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    ClientToServer,
    ServerToClient,
}

/// Appends one record to `file`. Fails when `bytes` is longer than a 32-bit length can say.
pub fn append(file: &mut Vec<u8>, direction: Direction, bytes: &[u8]) -> Result<(), String> {
    let length = u32::try_from(bytes.len())
        .map_err(|_| format!("a record of {} bytes is too long", bytes.len()))?;
    file.push(match direction {
        Direction::ClientToServer => 0,
        Direction::ServerToClient => 1,
    });
    file.extend(length.to_be_bytes());
    file.extend(bytes);
    Ok(())
}

/// The bytes of each record in `file`, in order. Fails on a direction other than 0 or 1, or a record that runs
/// past the end of the file.
pub fn records(file: &[u8]) -> Result<Vec<&[u8]>, String> {
    let mut found = Vec::new();
    let mut rest = file;
    while let [direction, a, b, c, d, tail @ ..] = rest {
        if *direction > 1 {
            return Err(format!("a record has direction {direction}"));
        }
        let length = u32::from_be_bytes([*a, *b, *c, *d]) as usize;
        let record = tail
            .get(..length)
            .ok_or("a record runs past the end of the file")?;
        found.push(record);
        rest = tail.get(length..).unwrap_or_default();
    }
    if rest.is_empty() {
        Ok(found)
    } else {
        Err("the file ends inside a record's length".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_is_the_direction_the_length_and_the_bytes() {
        let mut file = Vec::new();
        append(&mut file, Direction::ClientToServer, b"ab").unwrap();
        append(&mut file, Direction::ServerToClient, &[7; 300]).unwrap();
        let mut expected = vec![0, 0, 0, 0, 2, b'a', b'b', 1, 0, 0, 1, 44];
        expected.extend([7; 300]);
        assert_eq!(file, expected);
    }

    #[test]
    fn records_reads_back_what_append_wrote() {
        let mut file = Vec::new();
        append(&mut file, Direction::ServerToClient, b"abc").unwrap();
        append(&mut file, Direction::ClientToServer, b"").unwrap();
        append(&mut file, Direction::ClientToServer, b"de").unwrap();
        assert_eq!(records(&file), Ok(vec![&b"abc"[..], b"", b"de"]));
        assert_eq!(records(&[]), Ok(vec![]));
    }

    #[test]
    fn records_refuses_a_cut_record_or_an_unknown_direction() {
        let mut file = Vec::new();
        append(&mut file, Direction::ClientToServer, b"abc").unwrap();
        assert!(records(&file[..7]).is_err());
        assert!(records(&file[..3]).is_err());
        file[0] = 2;
        assert!(records(&file).is_err());
    }
}
