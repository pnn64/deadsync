use log::warn;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub struct CoalescedFileWriter {
    tx: Option<mpsc::Sender<WriteReq>>,
    path: PathBuf,
}

impl CoalescedFileWriter {
    pub fn new(thread_name: &str, path: PathBuf) -> Self {
        let tx = start_write_worker(thread_name, path.clone());
        Self { tx, path }
    }

    #[inline(always)]
    pub fn write(&self, content: String) {
        if let Some(tx) = self.tx.as_ref() {
            if let Err(err) = tx.send(WriteReq::Write(content))
                && let WriteReq::Write(content) = err.0
            {
                write_file(&self.path, &content);
            }
            return;
        }
        write_file(&self.path, &content);
    }

    pub fn flush(&self, timeout: Duration) {
        if let Some(tx) = self.tx.as_ref() {
            let (ack_tx, ack_rx) = mpsc::channel::<()>();
            if tx.send(WriteReq::Flush(ack_tx)).is_ok() {
                let _ = ack_rx.recv_timeout(timeout);
            }
        }
    }
}

enum WriteReq {
    Write(String),
    Flush(mpsc::Sender<()>),
}

fn start_write_worker(thread_name: &str, path: PathBuf) -> Option<mpsc::Sender<WriteReq>> {
    let (tx, rx) = mpsc::channel::<WriteReq>();
    let spawn = thread::Builder::new()
        .name(thread_name.to_string())
        .spawn(move || write_worker_loop(rx, path));
    match spawn {
        Ok(_) => Some(tx),
        Err(e) => {
            warn!("Failed to start {thread_name} thread: {e}. Falling back to sync writes.");
            None
        }
    }
}

fn write_worker_loop(rx: mpsc::Receiver<WriteReq>, path: PathBuf) {
    let mut pending_write: Option<String> = None;
    let mut flush_acks: Vec<mpsc::Sender<()>> = Vec::with_capacity(2);
    while let Ok(msg) = rx.recv() {
        match msg {
            WriteReq::Write(content) => pending_write = Some(content),
            WriteReq::Flush(ack) => flush_acks.push(ack),
        }
        while let Ok(msg) = rx.try_recv() {
            match msg {
                WriteReq::Write(content) => pending_write = Some(content),
                WriteReq::Flush(ack) => flush_acks.push(ack),
            }
        }
        if let Some(content) = pending_write.take() {
            write_file(&path, &content);
        }
        for ack in flush_acks.drain(..) {
            let _ = ack.send(());
        }
    }
    if let Some(content) = pending_write.take() {
        write_file(&path, &content);
    }
}

fn write_file(path: &Path, content: &str) {
    if let Err(e) = std::fs::write(path, content) {
        warn!("Failed to save '{}': {e}", path.display());
    }
}
