use topcoat::Result;
use topcoat::view::{view, component, Unescaped};
use topcoat::context::{Cx, app_context};

use crate::context::AppContext;

#[component]
pub async fn site_footer(cx: &Cx) -> Result {
    let ctx: &AppContext = app_context(cx);
    let footer = &ctx.settings.footer;
    let site_url = &ctx.settings.site.url;

    view! {
        <footer class="flex flex-col sm:flex-row" style="border-top: 1px solid var(--color-border-weak);">
            <div class="flex-1 text-center">
                <a href=(site_url) target="_blank" rel="noopener" class="block no-underline py-3 sm:py-4 text-xs transition" style="color: var(--color-text-weak);">(&footer.text)</a>
            </div>
            for link in &footer.links {
                <div class="flex-1 text-center sm:border-l" style="border-color: var(--color-border-weak);">
                    <a href=(&link.url) target="_blank" rel="noopener" class="block no-underline py-3 sm:py-4 text-xs transition" style="color: var(--color-text-weak);">(&link.label)</a>
                </div>
            }
        </footer>
        <style>{ (Unescaped::new_unchecked(r#"footer a:hover { background: var(--color-bg-weak); text-decoration: underline; }
        @media (max-width: 40rem) { footer .sm\:border-l { border-left: none !important; } }"#)) }</style>
    }
}
