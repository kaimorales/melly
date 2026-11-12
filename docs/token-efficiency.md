# Token efficiency baseline

Melly's conductor needs enough context to diagnose a task; Luna needs the brief and the files it edits. The Codex App Server also injects a general skill catalog and account memories into each thread. In a one-line coding task, those general sections occupied about 62,000 characters of the conductor's developer context and 45,500 of Luna's.

Melly now requests per-thread Codex configuration on start and resume. The conductor keeps memory and receives at most 3,000 tokens of automatic skill catalog. Workers receive at most 1,200 catalog tokens and skip personal memory injection. Their brief, project instructions, and tool access remain. Users can still attach skills to the conductor through `/skills`. These are context controls, not changes to model pricing or plan limits.

The same disposable task asked Sol at low effort to have Luna at low effort change `answer()` from 41 to 42, run `python3 check.py`, and review the diff. Both runs passed and recorded an accepted conductor review:

| One observed run | Before | After |
| --- | ---: | ---: |
| Conductor input tokens | 195,894 | 182,743 |
| Luna input tokens | 79,442 | 73,843 |
| Combined input tokens | 275,336 | 256,586 |
| Combined input tokens excluding cached input | 53,896 | 46,794 |
| Luna's general developer context | ~45,500 characters | ~11,900 characters |

That is about 7% fewer reported input tokens and 13% fewer non-cached input tokens in these two runs. Context, cache warmth, runtime instructions, and output varied between runs, so this is an observation rather than a general savings estimate. The after run was not faster. Reported tokens cannot be converted directly into Codex subscription usage. Keep testing representative tasks before tuning the caps further.
