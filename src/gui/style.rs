pub const CSS: &str = r#"
* {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
}

html, body {
    overflow: hidden;
    overscroll-behavior: none;
}

.app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: light-dark(#ffffff, #1e1e1e);
    color: light-dark(#1e1e1e, #d4d4d4);
    font-family: system-ui, -apple-system, sans-serif;
    font-size: 13px;
    color-scheme: light dark;
    overflow: hidden;
}

.toolbar {
    display: flex;
    gap: 16px;
    padding: 8px 12px;
    background: light-dark(#f3f3f3, #252526);
    border-bottom: 1px solid light-dark(#d4d4d4, #3c3c3c);
    flex-wrap: wrap;
    align-items: center;
}

.filter-group {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 200px;
}

.filter-group label {
    color: light-dark(#616161, #858585);
    font-size: 12px;
    min-width: 55px;
    flex-shrink: 0;
}

.filter-group input {
    background: light-dark(#ffffff, #3c3c3c);
    border: 1px solid light-dark(#c4c4c4, #4c4c4c);
    border-radius: 3px;
    color: light-dark(#1e1e1e, #d4d4d4);
    padding: 4px 8px;
    font-size: 12px;
    flex: 1;
    min-width: 150px;
}

.filter-group input:focus {
    outline: none;
    border-color: #007acc;
}

.filter-group input.error {
    border-color: #f44747;
}

.toolbar-actions {
    display: flex;
    gap: 6px;
    margin-left: auto;
}

.toolbar-actions button {
    background: light-dark(#e0e0e0, #3c3c3c);
    border: 1px solid light-dark(#c4c4c4, #4c4c4c);
    border-radius: 3px;
    color: light-dark(#1e1e1e, #d4d4d4);
    padding: 4px 10px;
    font-size: 12px;
    cursor: pointer;
}

.toolbar-actions button:hover {
    background: light-dark(#d0d0d0, #4c4c4c);
}

.toolbar-actions button.active {
    background: #007acc;
    border-color: #007acc;
    color: #ffffff;
}

.log-wrapper {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    position: relative;
}

.log-main {
    flex: 1;
    display: flex;
    flex-direction: row;
    overflow: hidden;
}

.log-container {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    background: light-dark(#ffffff, #1e1e1e);
    outline: none;
    position: relative;
    overscroll-behavior: contain;
}

.nowrap-mode.log-container {
    overflow-x: auto;
}

.log-container:focus {
    outline: none;
}

.log-list {
    position: relative;
    will-change: transform;
    padding-bottom: 4px;
}

.nowrap-mode .log-list {
    min-width: max-content;
}

.log-line {
    display: flex;
    padding: 1px 12px;
    font-family: 'SF Mono', Menlo, Monaco, 'Courier New', monospace;
    font-size: 12px;
    height: 20px;
    line-height: 18px;
}

.nowrap-mode .log-line {
    white-space: nowrap;
}

.wrap-mode .log-line {
    white-space: pre-wrap;
    word-break: break-all;
    height: auto;
    min-height: 20px;
}

.log-line:hover {
    background: light-dark(#f0f0f0, #2a2d2e);
}

.timestamp {
    margin-right: 12px;
    flex-shrink: 0;
    width: 32px;
    text-align: right;
}

.timestamp.very-recent {
    color: light-dark(#00aa00, #00ff00);
    font-weight: bold;
}

.timestamp.recent {
    color: light-dark(#098658, #4ec9b0);
}

.timestamp.minutes {
    color: light-dark(#666666, #888888);
}

.timestamp.hours {
    color: light-dark(#999999, #666666);
}

.timestamp.days {
    color: light-dark(#aaaaaa, #555555);
}

.line-num {
    color: light-dark(#858585, #858585);
    margin-right: 12px;
    min-width: 50px;
    text-align: right;
    flex-shrink: 0;
}

.content {
    color: light-dark(#1e1e1e, #d4d4d4);
}

.nowrap-mode .content {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.wrap-mode .content {
    white-space: pre-wrap;
    word-break: break-all;
}

.hl-error {
    color: light-dark(#dc3545, #f85149);
    font-weight: bold;
}

.hl-warn {
    color: light-dark(#ffc107, #d29922);
    font-weight: bold;
}

.hl-info {
    color: light-dark(#28a745, #3fb950);
    font-weight: bold;
}

.hl-debug {
    color: light-dark(#17a2b8, #58a6ff);
}

.hl-bracket {
    color: light-dark(#0066cc, #79c0ff);
}

.hl-timestamp {
    color: light-dark(#6f42c1, #d2a8ff);
}

.hl-custom {
    background: light-dark(#ffff00, #ffcc00);
    color: light-dark(#000000, #000000);
    padding: 0 2px;
    border-radius: 2px;
    font-weight: bold;
}

.hl-json-key {
    color: light-dark(#17a2b8, #58a6ff);
}

.hl-json-string {
    color: light-dark(#28a745, #3fb950);
}

.hl-json-number {
    color: light-dark(#fd7e14, #d29922);
}

.hl-json-bool {
    color: light-dark(#6f42c1, #d2a8ff);
}

.hl-json-null {
    color: light-dark(#dc3545, #f85149);
}

.scrollbar {
    width: 14px;
    background: light-dark(#f0f0f0, #1e1e1e);
    border-left: 1px solid light-dark(#d4d4d4, #3c3c3c);
    position: relative;
}

.scrollbar-thumb {
    position: absolute;
    width: 10px;
    left: 2px;
    background: light-dark(#c4c4c4, #5a5a5a);
    border-radius: 5px;
    min-height: 30px;
}

.scrollbar-thumb:hover {
    background: light-dark(#a0a0a0, #787878);
}

.scrollbar-h {
    height: 14px;
    background: light-dark(#f0f0f0, #1e1e1e);
    border-top: 1px solid light-dark(#d4d4d4, #3c3c3c);
    position: relative;
}

.scrollbar-thumb-h {
    position: absolute;
    height: 10px;
    top: 2px;
    background: light-dark(#c4c4c4, #5a5a5a);
    border-radius: 5px;
    min-width: 30px;
}

.scrollbar-thumb-h:hover {
    background: light-dark(#a0a0a0, #787878);
}

.statusbar {
    display: flex;
    justify-content: space-between;
    padding: 4px 12px;
    background: #007acc;
    color: #fff;
    font-size: 12px;
}

.statusbar.disconnected {
    background: #c42b1c;
}

.status-info {
    opacity: 0.9;
}

.status-msg {
    opacity: 0.8;
}

.popup-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
}

.popup {
    background: light-dark(#ffffff, #252526);
    border: 1px solid light-dark(#d4d4d4, #454545);
    border-radius: 8px;
    padding: 16px 20px;
    min-width: 320px;
    max-width: 500px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
}

.popup-header {
    font-size: 14px;
    margin-bottom: 12px;
    color: light-dark(#1e1e1e, #d4d4d4);
}

.popup-port {
    color: #e5c07b;
    font-weight: bold;
}

.popup-mode {
    font-size: 12px;
    margin-bottom: 4px;
}

.popup-label {
    color: light-dark(#858585, #858585);
}

.popup-mode-value {
    color: #e5c07b;
}

.popup-hint {
    font-size: 11px;
    color: light-dark(#858585, #858585);
    margin-bottom: 12px;
}

.popup-interfaces {
    max-height: 300px;
    overflow-y: auto;
}

.popup-error {
    color: #f44747;
    font-size: 12px;
}

.popup-interface {
    margin-bottom: 8px;
}

.popup-iface-name {
    font-size: 12px;
    font-weight: 500;
    color: #4ec9b0;
    margin-bottom: 4px;
}

.popup-iface-name.default {
    color: #6a9955;
}

.popup-addr {
    display: flex;
    align-items: center;
    padding: 4px 8px;
    font-family: 'SF Mono', Menlo, Monaco, 'Courier New', monospace;
    font-size: 12px;
    cursor: pointer;
    border-radius: 4px;
    margin-left: 8px;
}

.popup-addr:hover {
    background: light-dark(#e8e8e8, #3c3c3c);
}

.popup-addr.selected {
    background: light-dark(#007acc33, #007acc44);
}

.popup-addr.self-assigned {
    opacity: 0.5;
}

.popup-addr-indicator {
    color: #007acc;
    margin-right: 4px;
    width: 16px;
}

.popup-addr-text {
    color: light-dark(#1e1e1e, #d4d4d4);
}
"#;
