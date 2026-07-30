use topcoat::Result;
use topcoat::view::{view, component};

#[component]
pub async fn chat_message() -> Result {
    view! {
        <div class="flex">
            <div class="max-w-[75%] rounded-2xl bg-foreground/5 px-4 py-2 text-sm">
                <p>"Message content"</p>
            </div>
        </div>
    }
}
