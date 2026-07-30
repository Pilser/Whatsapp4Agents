use topcoat::Result;
use topcoat::view::{view, component};

#[component]
pub async fn site_footer() -> Result {
    view! {
        <footer class="border-t border-border">
            <div class="mx-auto max-w-6xl px-6 py-8">
                <p class="text-sm text-muted-foreground text-center">
                    "Built with "
                    <a href="https://topcoat.dev" class="underline hover:text-foreground transition-colors">"Topcoat"</a>
                    " and "
                    <a href="https://rust-lang.org" class="underline hover:text-foreground transition-colors">"Rust"</a>
                </p>
            </div>
        </footer>
    }
}
