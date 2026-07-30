use topcoat::Result;
use topcoat::view::{view, component};

#[component]
pub async fn site_header() -> Result {
    view! {
        <header class="border-b border-border">
            <nav class="mx-auto flex max-w-6xl items-center justify-between px-6 py-4">
                <a href="/" class="text-lg font-bold tracking-tight">"WAOS"</a>
                <div class="flex items-center gap-4">
                    <a href="/chat" class="text-sm text-muted-foreground hover:text-foreground transition-colors">"Chat"</a>
                    <a
                        href="/owner"
                        class="inline-flex h-8 items-center rounded-md border border-border px-3 text-xs font-medium shadow-xs transition-colors hover:bg-foreground/5"
                    >"Owner"</a>
                </div>
            </nav>
        </header>
    }
}
