use topcoat::Result;
use topcoat::view::{view, component};

use crate::components::card::{card, card_header, card_title, card_description};

const FAQ_ITEMS: &[(&str, &str)] = &[
    ("What is WAOS?", "WAOS (WhatsApp for Agents OS) is a platform that connects AI agents to WhatsApp."),
    ("How does it work?", "WAOS bridges WhatsApp Web with AI agent frameworks. Messages are routed to your agent and responses stream back in real-time."),
    ("Can I deploy my own agent?", "Yes. Deploy your own agent through the ZeroClaw bridge."),
    ("Is it open source?", "Yes. WAOS is fully open source under the MIT license."),
];

#[component]
pub async fn faq() -> Result {
    view! {
        <section class="mx-auto max-w-3xl px-6 py-20">
            <h2 class="text-3xl font-bold tracking-tight text-center">
                "Frequently asked questions"
            </h2>
            <div class="mt-10 flex flex-col gap-4">
                for (question, answer) in FAQ_ITEMS {
                    card(
                        card_header(
                            card_title((*question))
                            card_description((*answer))
                        )
                    )
                }
            </div>
        </section>
    }
}
