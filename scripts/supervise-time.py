#!/usr/bin/env python3
"""Time arithmetic for scripts/supervise.sh.

  supervise-time.py deadline [SPEC]   -> epoch seconds of the hard stop
      SPEC is "YYYY-MM-DD HH:MM" in local time; default is next Saturday 17:00.
  supervise-time.py reset-sleep TEXT  -> seconds to sleep given a Claude Code
      usage-limit message such as "You've hit your session limit · resets 3:45pm"
      or "... resets Mon 12:00am". Unparseable text yields 900.
"""
import datetime
import re
import sys

WEEKDAYS = {"mon": 0, "tue": 1, "wed": 2, "thu": 3, "fri": 4, "sat": 5, "sun": 6}


def deadline(spec: str) -> int:
    now = datetime.datetime.now()
    if spec:
        for fmt in ("%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M", "%Y-%m-%d"):
            try:
                return int(datetime.datetime.strptime(spec, fmt).timestamp())
            except ValueError:
                continue
        sys.exit(f"bad DEADLINE {spec!r}; use 'YYYY-MM-DD HH:MM'")
    days_ahead = (5 - now.weekday()) % 7
    target = (now + datetime.timedelta(days=days_ahead)).replace(hour=17, minute=0, second=0, microsecond=0)
    if target <= now:
        target += datetime.timedelta(days=7)
    return int(target.timestamp())


def reset_sleep(text: str) -> int:
    match = re.search(
        r"resets?\s+(?:(mon|tue|wed|thu|fri|sat|sun)\w*\s+)?(\d{1,2})(?::(\d{2}))?\s*(am|pm)",
        text,
        re.IGNORECASE,
    )
    if not match:
        return 900
    weekday, hour, minute, meridiem = match.groups()
    hour = int(hour) % 12 + (12 if meridiem.lower() == "pm" else 0)
    minute = int(minute or 0)
    now = datetime.datetime.now()
    target = now.replace(hour=hour, minute=minute, second=0, microsecond=0)
    if weekday:
        days_ahead = (WEEKDAYS[weekday.lower()] - now.weekday()) % 7
        target += datetime.timedelta(days=days_ahead)
    if target <= now:
        target += datetime.timedelta(days=7 if weekday else 1)
    # The message gives minutes at best; a short buffer avoids retrying one minute early.
    return max(60, int((target - now).total_seconds()) + 90)


if __name__ == "__main__":
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "deadline":
        print(deadline(sys.argv[2] if len(sys.argv) > 2 else ""))
    elif command == "reset-sleep":
        print(reset_sleep(sys.argv[2] if len(sys.argv) > 2 else ""))
    else:
        sys.exit(__doc__)
