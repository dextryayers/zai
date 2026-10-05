# UX

Tokens: accent cyan, ok green, warn amber, danger red, muted gray.
Borders: rounded unicode when TTY and width >= 80, else ASCII.
Status line is one row and never wraps. Hint line lists active keys.
Tables truncate paths from left to keep filename visible.
Markdown supports headings, lists, code fences with language tag, inline code, bold, links as footnotes.
Streaming shows caret and flushes in small batches for elegant pacing.
Pipe mode and --plain disable color. --json emits envelope only on stdout.
Fallback verified at 80x24, NO_COLOR=1, TERM=dumb.
