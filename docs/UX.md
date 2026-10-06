# UX

Premium modern CLI. Tokens: accent cyan, accent2 purple, ok green, warn amber, danger red, muted gray.
Borders: rounded unicode when TTY and width >= 80, else ASCII.
Status bar is one row and never wraps. Format: ZAI badge plus version plus model plus effort plus ctx meter plus offline plus profile.
Chat cards: YOU uses green marker with left gutter, ZAI uses cyan marker with left gutter, system uses muted divider.
Welcome: centered block font ZAI plus Welcome to Zai plus subtitle plus quick start. Shown on boot splash and on empty chat.
Slash palette: type / to see all commands. Up and Down move selection. Tab or Enter applies the highlighted command. Esc dismisses.
Commands: /help /new /new-chat /sessions /open /model /insert /manage /ollama /run /setting /effort /budget /compact /export /sources /clear /plain /quit.
Behavior: /new-chat starts a fresh chat session. /clear clears the entire chat log view and restores the welcome banner.
Shortcuts: Ctrl+N new chat, Ctrl+L clear log, Ctrl+P palette, Ctrl+O models, Ctrl+E effort, Ctrl+B budget, Ctrl+S settings, Ctrl+U clear input, F1 help, F2 models, F3 focus toggle, Alt+1 sessions, Alt+2 chat, Up and Down history or slash pick, Tab complete, PgUp and PgDn scroll, Ctrl+C stop, Ctrl+D exit.
REPL: centered welcome art, Tab completes slash commands, Up and Down recalls history, Ctrl+A and Ctrl+E move in line, Ctrl+U clears line, Ctrl+L clears screen, Ctrl+R searches history.
Markdown supports headings, lists, code fences with language tag, inline code, bold, links as footnotes.
Streaming shows caret and flushes in small batches for elegant pacing.
Pipe mode and --plain disable color. --json emits envelope only on stdout.
Fallback verified at 80x24, NO_COLOR=1, TERM=dumb.
