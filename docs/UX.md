# UX

Premium modern CLI on a uniform dark slate stage, never the terminal default background, so light terminals can not leak white anywhere including popups and gauges. Tokens: accent cyan, accent2 purple, ok green, warn amber, danger red, muted gray.
Borders: rounded unicode when TTY and width >= 80, else ASCII.
Every launch opens a totally clean chat: the last session is reused only when it never got a turn, otherwise a brand new session starts. Older chats stay untouched in the sessions tabs.
First run opens minimal: hero ZAI block logo plus a bordered quick start card plus a full width chat column plus input plus model bar. No side tabs yet.
First Enter with a message flips to full: sessions tabs left, chat main, input, model bar. Empty views (/new-chat, /clear, empty /open) return to minimal.
Content column is centered, max 124 wide with gutters on wide terminals.
History is exact: the question persists the moment Enter is pressed, the answer persists when its stream finishes, and Ctrl+C mid stream saves the real visible prefix as stopped. Enter during a stream never drops text.
Top line is a slim brand strip only. The model bar always sits below the chat column on two lines: line one carries the FULL GGUF file name from the registry, line two quant plus size plus effort plus ctx meter plus profile plus turn count plus session tag. Compact form on narrow screens.
Chat cards: YOU uses green marker with left gutter, numbered per exchange, ZAI uses cyan marker with left gutter plus the answering model tag, system uses muted divider. Every card header carries the turn time. A scrollbar appears on the chat edge whenever scrolled up.
History renders the last 20 turns with a labeled cut plus a legacy mock cleanup note when old stored turns contained early mock artifacts.
Welcome: block font ZAI with a cyan to magenta gradient plus Welcome to Zai plus subtitle plus quick start. Shown on boot splash, minimal stage hero, and empty chat.
Quick start card: bordered panel with type, /model, /insert, /history, /help plus the ready model line. Never a plain screen.
Slash palette: type / to see all commands. Up and Down move selection. Tab or Enter applies the highlighted command. Esc dismisses.
Commands: /help /new /new-chat /sessions /open /model /insert /add /manage /ollama /run /setting /effort /budget /compact /history /export /sources /clear /plain /quit.
Behavior: /new-chat starts a fresh chat session. /clear clears the entire chat log view and restores the minimal stage. /history lists recent exchanges compactly.
Shortcuts: Ctrl+N new chat, Ctrl+L clear log, Ctrl+P palette, ? help when input is empty, Ctrl+O models, Ctrl+E effort, Ctrl+B budget, Ctrl+S settings, Ctrl+U clear input, Ctrl+A line start, Ctrl+K kill to cursor end, Ctrl+W delete word, Home and End jump, Delete forward delete, F1 help, F2 models, F3 focus toggle, Alt+1 sessions, Alt+2 chat, Up and Down history or slash pick, Tab complete, PgUp and PgDn scroll, Ctrl+C stop, Ctrl+D exit.
REPL: centered welcome art, Tab completes slash commands, Up and Down recalls history, Ctrl+A and Ctrl+E move in line, Ctrl+U clears line, Ctrl+L clears screen, Ctrl+R searches history.
Models: `zai insert <file> [more...]` guesses name, quant, ctx. Folders scan with `--recursive`. `zai models scan` finds files across default folders. `zai models inspect <file>` reads one file deep.
Markdown supports headings, lists, code fences with language tag, inline code, bold, links as footnotes.
Streaming shows caret and flushes in small batches for elegant pacing.
Pipe mode and --plain disable color. --json emits envelope only on stdout.
Fallback verified at 80x24, NO_COLOR=1, TERM=dumb.
