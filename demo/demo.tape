# lens demo — run via ./demo/record.sh (it sets the LENS_DEMO_* variables).

Output assets/demo.gif
# Text transcript of every frame — record.sh scans it for leaked paths/usernames.
Output demo/.tmp/demo.ascii

Require lens

Set Shell "bash"
Set Theme "Catppuccin Mocha"
Set FontFamily "JetBrains Mono"
Set FontSize 16
Set Width 1500
Set Height 720
Set Padding 20
Set TypingSpeed 60ms

# --- Hidden setup: isolated env, neutral prompt, fixture deps -----------------
Hide
Type `exec env -i HOME="$LENS_DEMO_HOME" PATH="$LENS_DEMO_PATH" LENS_DEMO_WORKDIR="$LENS_DEMO_WORKDIR" TERM=xterm-256color LANG=en_US.UTF-8 EDITOR=vim bash --noprofile --norc`
Enter
Sleep 500ms
Type `PS1='\[\e[1;35m\]shop\[\e[0m\] $ '; cd "$LENS_DEMO_WORKDIR" && npm install --no-audit --no-fund --silent && clear`
Enter
Wait+Screen@60s /shop \$/
Sleep 300ms
Show

# 1. Launch in watch mode
Sleep 800ms
Type "lens -w"
Sleep 500ms
Enter
Wait+Screen@20s /4 files/
Sleep 1s

# 2. Run everything — two failures appear
Type "a"
Wait+Screen@30s /✘ 2/
Sleep 1.5s

# 3. Jump to the first failure: structured expected/actual diff
Type "]"
Sleep 4s

# 4. Next failure (same root cause, string diff), then back
Type "]"
Sleep 2.5s
Type "["
Sleep 1s

# 5. Open in editor at the failure and fix the bug in cart.ts
Type "e"
Sleep 1.5s
Type ":e src/cart.ts"
Sleep 300ms
Enter
Sleep 1s
Type "/ + tax)"
Sleep 300ms
Enter
Sleep 1s
Type "dt)"
Sleep 1.2s
Type ":wq"
Sleep 300ms
Enter

# 6. Watch mode re-runs automatically — all green
Wait+Screen@30s /✘ 0/
Sleep 3.5s

# 7. Quit
Type "q"
Sleep 800ms
