use topcoat::Result;
use topcoat::view::{view, component};

use crate::components::button::{ButtonSize, ButtonVariant, button_variants};

#[component]
pub async fn hero() -> Result {
    view! {
        <section class="mx-auto max-w-6xl px-6 py-24 text-center">
            <h1 class="text-5xl font-bold tracking-tight">
                "WhatsApp "
                <span class="text-primary">"for Agents"</span>
            </h1>
            <p class="mx-auto mt-4 max-w-2xl text-lg text-muted-foreground">
                "Connect your AI agents to WhatsApp. Deploy, monitor, and manage \
                 intelligent conversations at scale."
            </p>
            <div class="mt-8 flex items-center justify-center gap-4">
                <a
                    href="/chat"
                    class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
                >
                    "Try the Chat"
                </a>
                <a
                    href="https://github.com/Pilser/Whatsapp4Agents"
                    class=(button_variants(ButtonVariant::Outline, ButtonSize::Lg))
                >
                    "View on GitHub"
                </a>
            </div>
        </section>
    }
}
