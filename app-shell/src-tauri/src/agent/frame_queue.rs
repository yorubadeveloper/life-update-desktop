//! Bounded in-memory queue for screenshots awaiting a (slow) vision-model
//! description. Never touches disk; oldest frame is evicted when full.
//! Port of the Python agent's capture/frame_queue.py.

use std::collections::VecDeque;
use std::sync::Mutex;

const MAX_FRAMES: usize = 20;

pub struct PendingFrame {
    pub png_bytes: Vec<u8>,
    pub app_name: Option<String>,
    pub title: Option<String>,
    pub ts: String,
}

#[derive(Default)]
pub struct FrameQueue(Mutex<VecDeque<PendingFrame>>);

impl FrameQueue {
    pub fn push(&self, frame: PendingFrame) {
        let mut q = self.0.lock().unwrap();
        if q.len() >= MAX_FRAMES {
            q.pop_front();
        }
        q.push_back(frame);
    }

    pub fn drain(&self) -> Vec<PendingFrame> {
        self.0.lock().unwrap().drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ts: &str) -> PendingFrame {
        PendingFrame { png_bytes: vec![1], app_name: None, title: None, ts: ts.into() }
    }

    #[test]
    fn evicts_oldest_when_full() {
        let q = FrameQueue::default();
        for i in 0..25 {
            q.push(frame(&i.to_string()));
        }
        assert_eq!(q.len(), MAX_FRAMES);
        let drained = q.drain();
        assert_eq!(drained[0].ts, "5"); // 0-4 evicted
        assert_eq!(q.len(), 0);
    }
}
