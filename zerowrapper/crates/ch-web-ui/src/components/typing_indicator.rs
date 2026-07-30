use topcoat::Result;
use topcoat::view::{view, component};

#[component]
pub async fn typing_indicator() -> Result {
    view! {
        <div class="flex items-center gap-1 px-4 py-2">
            <div class="flex items-center gap-1">
                <span class="size-1.5 rounded-full bg-muted-foreground"></span>
                <span class="size-1.5 rounded-full bg-muted-foreground"></span>
                <span class="size-1.5 rounded-full bg-muted-foreground"></span>
            </div>
            <span class="text-xs text-muted-foreground">"typing..."</span>
        </div>
    }
}
