use topcoat::Result;
use topcoat::view::view;
use topcoat::view::component;

use crate::components::input::input;
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};

#[component]
pub async fn chat_app() -> Result {
    view! {
        <div class="mx-auto flex h-dvh max-w-2xl flex-col bg-background">
            <header class="flex items-center justify-between border-b border-border px-4 py-3">
                <div class="flex items-center gap-3">
                    <div class="size-2 rounded-full bg-primary"></div>
                    <div>
                        <p class="text-sm font-semibold">"WAOS Chat"</p>
                        <p class="text-xs text-muted-foreground">"Connected"</p>
                    </div>
                </div>
                <div class="flex items-center gap-2">
                    <span class="text-xs text-muted-foreground" id="user-info"></span>
                    <button
                        class=(button_variants(ButtonVariant::Outline, ButtonSize::Sm))
                        id="auth-btn"
                    >
                        "Sign in"
                    </button>
                </div>
            </header>

            <div id="messages" class="flex-1 space-y-3 overflow-y-auto px-4 py-4">
            </div>

            <div class="border-t border-border px-4 py-3">
                <div class="flex items-end gap-2">
                    <div class="flex-1">
                        input(
                            attrs: topcoat::view::attributes! {
                                id="chat-input"
                                placeholder="Type a message..."
                            }
                        )
                    </div>
                    <button
                        class=(button_variants(ButtonVariant::Primary, ButtonSize::Icon))
                        id="send-btn"
                    >
                        "Send"
                    </button>
                </div>
            </div>
        </div>
    }
}
