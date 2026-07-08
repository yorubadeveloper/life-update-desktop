from life_update_agent.capture.screen_watcher import should_capture


def test_captures_on_first_run_no_prior_window():
    assert should_capture(("App", "title"), None, now=100.0, last_capture_at=0.0, interval_seconds=120) is True


def test_captures_when_window_changes_even_if_interval_not_elapsed():
    assert should_capture(("App", "new title"), ("App", "old title"), now=101.0, last_capture_at=100.0, interval_seconds=120) is True


def test_captures_when_interval_elapsed_even_if_window_unchanged():
    assert should_capture(("App", "title"), ("App", "title"), now=221.0, last_capture_at=100.0, interval_seconds=120) is True


def test_does_not_capture_when_window_unchanged_and_interval_not_elapsed():
    assert should_capture(("App", "title"), ("App", "title"), now=150.0, last_capture_at=100.0, interval_seconds=120) is False
