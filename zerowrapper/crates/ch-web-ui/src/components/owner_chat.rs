use topcoat::Result;
use topcoat::view::{view, component, Unescaped};

#[component]
pub async fn owner_chat() -> Result {
    view! {
        <div id="owner-chat" class="hidden flex flex-col min-h-screen" style="background: var(--color-bg);">
            <div class="flex items-center justify-between px-4 py-3" style="flex-shrink: 0; border-bottom: 1px solid var(--color-border-weak);">
                <div class="flex items-center gap-2">
                    <span class="text-sm font-bold" style="color: var(--color-text-strong);">"Owner Chat"</span>
                    <span id="owner-role-badge" class="text-xs px-2 py-0.5 rounded" style="background: var(--color-bg-interactive); color: var(--color-text-inverted);"></span>
                </div>
                <button id="owner-back-btn" class="text-xs px-3 py-1.5 rounded cursor-pointer transition" style="color: var(--color-text-weak); border: 1px solid var(--color-border-weak); background: transparent;">"Back to Public"</button>
            </div>
            <div id="owner-messages" class="flex-1 overflow-y-auto px-4 py-4">
                <div id="owner-hint" class="flex flex-col items-center justify-center py-10 gap-3">
                    <p id="owner-hint-text" class="text-xs text-center" style="color: var(--color-text-weak);">"Connecting to backend…"</p>
                    <button id="owner-refresh-btn" class="hidden flex items-center gap-1.5 px-3 py-1.5 rounded text-xs cursor-pointer transition" style="color: var(--color-text-weak); border: 1px solid var(--color-border-weak); background: var(--color-bg-weak);" type="button">
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>
                        "Refresh"
                    </button>
                </div>
            </div>
            <div id="owner-typing" class="hidden px-4 py-2">
                <div class="flex gap-1.5 items-center" style="padding: 12px 0 8px 52px;">
                    <span class="w-3 h-3 rounded-full" style="background: var(--color-text-weak); animation: owner-typing-bounce 1.4s infinite ease-in-out both;"></span>
                    <span class="w-3 h-3 rounded-full" style="background: var(--color-text-weak); animation: owner-typing-bounce 1.4s infinite ease-in-out both; animation-delay: 0.16s;"></span>
                    <span class="w-3 h-3 rounded-full" style="background: var(--color-text-weak); animation: owner-typing-bounce 1.4s infinite ease-in-out both; animation-delay: 0.32s;"></span>
                </div>
            </div>
            <div style="flex-shrink: 0; border-top: 1px solid var(--color-border-weak);">
                <div style="padding: 8px 16px 4px;">
                    <div id="owner-attachments" class="flex flex-wrap gap-1 mb-1"></div>
                </div>
                <form id="owner-chat-form" style="padding: 4px 16px 12px;">
                    <div class="flex gap-2 items-end">
                        <button type="button" id="owner-upload-btn" class="flex items-center justify-center cursor-pointer transition rounded-lg flex-shrink-0" style="width: 36px; height: 36px; border: 1.5px solid var(--color-border); background: var(--color-bg-weak); color: var(--color-text-weak); margin-bottom: 2px;" title="Upload file">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
                        </button>
                        <input type="file" id="owner-file-input" class="hidden" multiple="true" />
                        <div class="flex-1 relative">
                            <textarea id="owner-chat-input" placeholder="Type your message here..." rows="2" class="w-full px-5 py-3.5 pr-12 text-sm rounded-xl resize-none outline-none transition leading-relaxed" style="min-height: 52px; max-height: 160px; border: 1.5px solid var(--color-border); background: var(--color-bg-weak); color: var(--color-text-strong); font-family: var(--font-mono);"></textarea>
                            <button type="submit" id="owner-send-btn" class="absolute bottom-2 right-2 z-10 flex items-center justify-center cursor-pointer transition rounded-lg" style="width: 32px; height: 32px; border: none; background: var(--color-bg-interactive); color: var(--color-text-inverted);">
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0;"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>
                            </button>
                            <button type="button" id="owner-cancel-btn" class="absolute bottom-2 right-2 z-10 hidden items-center justify-center cursor-pointer transition rounded-lg" style="width: 32px; height: 32px; border: none; background: var(--color-status-error); color: #fff;">
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/></svg>
                            </button>
                        </div>
                    </div>
                </form>
            </div>
        </div>
        <style>{ (Unescaped::new_unchecked(r#"@keyframes owner-typing-bounce { 0%, 80%, 100% { transform: scale(0); } 40% { transform: scale(1); } }
#owner-back-btn:hover { background: var(--color-bg-weak-hover) !important; color: var(--color-text) !important; }
#owner-chat-input:focus { border-color: var(--color-bg-interactive) !important; box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-bg-interactive) 30%, transparent) !important; }
#owner-chat-input::placeholder { color: var(--color-text-weak); opacity: 0.7; }
#owner-messages > .message-row { display: flex; flex-direction: column; margin-bottom: 16px; animation: msg-slide-in 0.2s ease-out; }
@keyframes msg-slide-in { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: translateY(0); } }
#owner-messages > .message-row.user { align-items: flex-end; }
#owner-messages > .message-row.assistant { align-items: flex-start; }
#owner-messages > .message-row .msg-avatar { width: 24px; height: 24px; border-radius: 50%; display: flex; align-items: center; justify-content: center; margin-bottom: 4px; }
#owner-messages > .message-row.user .msg-avatar { background: var(--color-bg-interactive); border: 1px solid var(--color-border-weak); }
#owner-messages > .message-row.assistant .msg-avatar { background: var(--color-bg-weak); border: 1px solid var(--color-border-weak); }
#owner-messages > .message-row .msg-avatar svg { width: 12px; height: 12px; }
#owner-messages > .message-row.user .msg-avatar svg { stroke: var(--color-text-inverted); }
#owner-messages > .message-row.assistant .msg-avatar svg { stroke: var(--color-text-weak); }
#owner-messages > .message-row .msg-content { padding: 12px 16px; font-size: 0.875rem; line-height: 1.75; word-wrap: break-word; border-radius: 10px; width: 100%; box-sizing: border-box; }
#owner-messages > .message-row.user .msg-content { border: 1px solid var(--color-border-weak); color: var(--color-text-inverted); font-weight: 450; background: var(--color-bg-interactive); border-bottom-right-radius: 4px; max-width: 75%; width: auto; }
#owner-messages > .message-row.assistant .msg-content { border: 1px solid var(--color-border-weak); color: var(--color-text); background: var(--color-bg); border-bottom-left-radius: 4px; }
#owner-messages .msg-attachments { display: flex; flex-wrap: wrap; gap: 4px; margin-bottom: 4px; max-width: 100%; overflow: hidden; }
#owner-messages .attachment-chip { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; font-size: 0.75rem; border-radius: 4px; border: 1px solid var(--color-border-weak); background: var(--color-bg-weak); color: var(--color-text); max-width: 100%; overflow: hidden; word-break: break-all; hyphens: auto; }
#owner-messages > .message-row.user .msg-content p { word-break: break-word; overflow-wrap: break-word; white-space: pre-wrap; }
#owner-messages .tool-call-details { font-size: 0.8rem; }
#owner-messages .tool-call-details summary { cursor: pointer; padding: 2px 0; user-select: none; }
#owner-messages .tool-call-details pre { font-size: 0.7rem; opacity: 0.7; margin: 4px 0 0; white-space: pre-wrap; overflow-x: auto; max-height: 200px; border-top: 1px solid var(--color-border-weak); padding-top: 4px; }
#owner-messages .tc-pill-status { display: inline-block; width: 12px; text-align: center; }
#owner-messages .tc-pill-status.running { animation: owner-spin 1s linear infinite; }
#owner-messages .tc-pill-status.done { color: var(--color-status-success, #34c759); }
#owner-messages .thinking-block { margin-bottom: 8px; }
#owner-messages .thinking-block details { font-size: 0.8rem; }
#owner-messages .thinking-block summary { cursor: pointer; user-select: none; opacity: 0.6; display: flex; align-items: center; gap: 4px; }
#owner-messages .thinking-block pre { font-size: 0.7rem; opacity: 0.5; margin: 4px 0 0; white-space: pre-wrap; overflow-x: auto; max-height: 200px; }
#owner-messages .think-icon { width: 14px; height: 14px; flex-shrink: 0; }
@keyframes owner-spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }"#)) }</style>
    }
}
