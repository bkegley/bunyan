# Full Editor Integration (Future)

The simple version (implemented) keeps tmux as the session engine and just opens a folder in the editor alongside. A full version would:

1. **Per-editor session management**: For IDE targets, skip tmux entirely. Open Claude/shell in the IDE's integrated terminal instead of tmux panes.
2. **Replace pane tracking**: The sidebar pane list (Running section) is entirely tmux-based. For IDE mode, either drop it or use a different mechanism (process monitoring, IDE extension).
3. **IDE extensions/plugins**: Build VSCode/Cursor/Zed extensions that communicate with bunyan to report session state back.
4. **Cross-platform**: The current AppleScript approach is macOS-only. Full version would need platform-specific launchers (xdg-open on Linux, start on Windows).
5. **Per-workspace editor override**: Allow different workspaces to use different editors (stored on the workspace model, not just a global setting).
6. **Deep integration**: Open specific files, position cursor, open integrated terminal with specific command — each IDE has different capabilities here.
