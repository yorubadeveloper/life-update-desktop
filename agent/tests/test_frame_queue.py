from life_update_agent.capture.frame_queue import FrameQueue, PendingFrame


def _frame(tag: str) -> PendingFrame:
    return PendingFrame(png_bytes=tag.encode(), app_name="App", title=tag, ts="2026-07-08T00:00:00+00:00")


def test_drain_returns_all_pushed_in_order():
    q = FrameQueue(maxlen=10)
    q.push(_frame("a"))
    q.push(_frame("b"))
    assert [f.title for f in q.drain()] == ["a", "b"]


def test_drain_empties_the_queue():
    q = FrameQueue(maxlen=10)
    q.push(_frame("a"))
    q.drain()
    assert len(q) == 0
    assert q.drain() == []


def test_bounded_drops_oldest_when_full():
    q = FrameQueue(maxlen=3)
    for tag in ["a", "b", "c", "d"]:
        q.push(_frame(tag))
    assert [f.title for f in q.drain()] == ["b", "c", "d"]


def test_len_reflects_current_size():
    q = FrameQueue(maxlen=5)
    assert len(q) == 0
    q.push(_frame("a"))
    assert len(q) == 1
