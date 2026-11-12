# Speed: reduce delegation round trips

The 0.7.1 live trace spent most of its elapsed time in model calls. After spawning a worker, the conductor made another call to wait, then another to resume a tool wrapper that had yielded after one second. The worker was already running during those calls.

Version 0.7.2 adds optional `wait_for_result` to `melly_agents.spawn`. When true, Melly starts the worker normally and holds the reply until the result arrives, or returns its status after 120 seconds. This reuses the event-driven wait, report delivery, quota, duplicate-request and cancellation handling. It does not block terminal input or worker events. Omitting the field preserves the immediate response for older chats and for parallel assignments. The conductor is also instructed to allow a waiting tool wrapper to run for up to 60 seconds before yielding.

The conductor still diagnoses the task before handing it off, inspects the actual diff afterward, and records a review. The same Sol low / Luna low disposable one-line fix passed its test and an accepted review before and after:

| Observed measure | 0.7.1 | 0.7.2 candidate |
| --- | ---: | ---: |
| Conductor model calls | 7 | 5 |
| Luna model calls | 4 | 4 |
| Session log elapsed time | 25.289 s | 22.878 s |
| Combined reported input tokens | 256,586 | 205,320 |
| Combined cached input tokens | 209,792 | 147,456 |
| Combined non-cached input tokens | 46,794 | 57,864 |

Elapsed time uses the same session-log start/end boundaries, excluding CLI startup. This is one historical run and one candidate run, not a controlled repeated benchmark. Luna's own duration also varied. The trace confirms two eliminated conductor calls; it does not establish a general percentage speedup or subscription saving. Cache differences mean non-cached input increased in the candidate run despite fewer total input tokens.

New chats expose the optional field. Old Codex threads retain their original tool schema, so their separate spawn/wait path remains supported.
