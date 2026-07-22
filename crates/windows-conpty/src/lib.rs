//! Safe Windows ConPTY ownership boundary.

#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{ConPtyChild, ConPtyControl, ConPtyReader, ConPtyWriter, SpawnedConPty, spawn};

#[cfg(all(test, windows))]
use windows::{create_test_pipe, test_output_drain as start_output_drain};

#[cfg(all(test, windows))]
mod tests {
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};

    use super::{create_test_pipe, start_output_drain};

    #[test]
    fn blocked_output_drain_is_cancelled_and_joined_before_deadline() {
        let (read_end, _writer_kept_open) = create_test_pipe().expect("test pipe");
        let (mut output, mut drain) = start_output_drain(read_end).expect("output drain");
        assert!(!drain.wait_for(Duration::from_millis(20)));

        let deadline = Instant::now() + Duration::from_millis(500);
        drain
            .close_before(deadline)
            .expect("cancel and join blocked drain");
        assert!(Instant::now() <= deadline);

        let mut bytes = Vec::new();
        output.read_to_end(&mut bytes).expect("drained output EOF");
        assert!(bytes.is_empty());
    }

    #[test]
    fn output_drain_preserves_multiple_chunks_and_exact_tail_through_eof() {
        let (read_end, mut writer) = create_test_pipe().expect("test pipe");
        let (mut output, mut drain) = start_output_drain(read_end).expect("output drain");
        let first = vec![b'a'; 32 * 1_024];
        let second = vec![b'b'; 17 * 1_024];
        let tail = b"|exact-final-tail|";
        writer.write_all(&first).expect("first chunk");
        writer.write_all(&second).expect("second chunk");
        writer.write_all(tail).expect("tail");
        drop(writer);

        let deadline = Instant::now() + Duration::from_secs(1);
        drain.close_before(deadline).expect("join normal drain");
        let mut actual = Vec::new();
        output.read_to_end(&mut actual).expect("drained output");

        let mut expected = first;
        expected.extend(second);
        expected.extend(tail);
        assert_eq!(actual, expected);
    }
}
