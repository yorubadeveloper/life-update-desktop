"""The first safety layer - checked *before* anything is captured.

Anything matching an excluded app name or window-title pattern is never
turned into an event at all, so it never reaches the redaction scanner or
local storage.
"""

from __future__ import annotations

import re

from life_update_agent.config import ExcludeList


def is_excluded(exclude_list: ExcludeList, app_name: str | None, window_title: str | None) -> bool:
    if app_name:
        lowered = app_name.lower()
        for excluded_app in exclude_list.apps:
            if excluded_app.lower() in lowered:
                return True

    if window_title:
        for pattern in exclude_list.title_patterns:
            if re.search(pattern, window_title):
                return True

    return False
