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
}
