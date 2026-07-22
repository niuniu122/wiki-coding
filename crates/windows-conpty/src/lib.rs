//! Safe Windows ConPTY ownership boundary.

#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{ConPtyChild, ConPtyControl, ConPtyReader, ConPtyWriter, SpawnedConPty, spawn};

#[cfg(all(test, windows))]
use windows::{
    OUTPUT_QUEUE_CAPACITY_BYTES, create_test_pipe, spawn_test_child,
    test_output_drain as start_output_drain,
    test_output_drain_paused_between_reads as start_output_drain_paused_between_reads,
};

#[cfg(all(test, windows))]
mod tests {
    use std::io::{Read, Write};
    use std::process::Command;
    use std::time::{Duration, Instant};

    use super::{
        OUTPUT_QUEUE_CAPACITY_BYTES, create_test_pipe, spawn_test_child, start_output_drain,
        start_output_drain_paused_between_reads,
    };

    #[test]
    fn output_queue_is_byte_bounded_and_receiver_drop_unblocks_the_producer() {
        let (read_end, mut writer) = create_test_pipe().expect("test pipe");
        let (output, mut drain) = start_output_drain(read_end).expect("output drain");
        let writer_thread = std::thread::spawn(move || {
            let payload = vec![b'q'; OUTPUT_QUEUE_CAPACITY_BYTES * 4];
            let result = writer.write_all(&payload);
            drop(writer);
            result
        });

        assert!(drain.wait_for_queue_full(Duration::from_secs(1)));
        assert!(drain.queued_bytes() <= OUTPUT_QUEUE_CAPACITY_BYTES);
        drop(output);

        writer_thread
            .join()
            .expect("bounded queue writer")
            .expect("receiver drop wakes producer and permits pipe drain");
        drain
            .close_before(Instant::now() + Duration::from_secs(1))
            .expect("drain joins after receiver drop");
    }

    #[test]
    fn full_output_queue_unblocks_on_cancellation_and_keeps_buffered_bytes() {
        let (read_end, mut writer) = create_test_pipe().expect("test pipe");
        let (mut output, mut drain) = start_output_drain(read_end).expect("output drain");
        let writer_thread = std::thread::spawn(move || {
            let payload = vec![b'c'; OUTPUT_QUEUE_CAPACITY_BYTES * 4];
            let _ = writer.write_all(&payload);
        });

        assert!(drain.wait_for_queue_full(Duration::from_secs(1)));
        drain
            .close_before(Instant::now() + Duration::from_millis(500))
            .expect("queue cancellation joins producer");
        writer_thread.join().expect("cancelled queue writer");

        let mut buffered = Vec::new();
        output
            .read_to_end(&mut buffered)
            .expect("buffered output remains readable");
        assert_eq!(buffered.len(), OUTPUT_QUEUE_CAPACITY_BYTES);
    }

    #[test]
    fn cancellation_between_reads_retries_until_next_read_and_preserves_tail() {
        let (read_end, mut writer) = create_test_pipe().expect("test pipe");
        let (mut output, drain, between_reads) =
            start_output_drain_paused_between_reads(read_end).expect("paused output drain");
        let first = b"first-frame|";
        let tail = b"exact-tail-after-between-read-cancel|";
        writer.write_all(first).expect("first frame");
        between_reads.wait_until_paused(Duration::from_secs(1));
        writer.write_all(tail).expect("tail while reader paused");

        let closer = std::thread::spawn(move || {
            let mut drain = drain;
            drain.close_before(Instant::now() + Duration::from_millis(500))
        });
        between_reads.wait_until_cancellation_requested(Duration::from_secs(1));
        between_reads.release();
        closer
            .join()
            .expect("between-read closer")
            .expect("repeated cancellation joins next read");
        drop(writer);

        let mut actual = Vec::new();
        output.read_to_end(&mut actual).expect("preserved frames");
        assert_eq!(actual, [first.as_slice(), tail.as_slice()].concat());
    }

    #[test]
    fn completed_process_exit_code_259_is_not_treated_as_still_running() {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "exit 259",
        ]);
        let mut child = spawn_test_child(&mut command).expect("spawn exit-259 process");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait().expect("poll exit-259 process") {
                Some(code) => {
                    assert_eq!(code, 259);
                    break;
                }
                None if Instant::now() < deadline => std::thread::yield_now(),
                None => panic!("literal exit 259 was misclassified as STILL_ACTIVE"),
            }
        }
    }

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
        let first = vec![b'a'; OUTPUT_QUEUE_CAPACITY_BYTES];
        let second = vec![b'b'; OUTPUT_QUEUE_CAPACITY_BYTES];
        let tail = b"|exact-final-tail|".to_vec();
        let expected = [first.clone(), second.clone(), tail.clone()].concat();
        let consumer = std::thread::spawn(move || {
            let mut actual = Vec::new();
            output.read_to_end(&mut actual).expect("drained output");
            actual
        });
        writer.write_all(&first).expect("first chunk");
        writer.write_all(&second).expect("second chunk");
        writer.write_all(&tail).expect("tail");
        drop(writer);

        let deadline = Instant::now() + Duration::from_secs(1);
        drain.close_before(deadline).expect("join normal drain");
        let actual = consumer.join().expect("output consumer");
        assert_eq!(actual, expected);
    }
}
