use topcoat::Result;
use topcoat::view::{view, component, Unescaped};
use topcoat::context::{Cx, app_context};

use crate::context::AppContext;
use crate::components::owner_login::owner_login;

#[component]
pub async fn chat_section(cx: &Cx) -> Result {
    let ctx: &AppContext = app_context(cx);
    let brand = ctx.settings.site.brand_or_title();

    view! {
        <section class="px-6 py-8 md:px-10 mb-2 md:mb-4" style="border-top: 1px solid var(--color-border-weak);">
            <h3 class="text-sm font-bold mb-2" style="color: var(--color-text-strong);">"Ask AI"</h3>
            <p class="text-sm mb-4" style="color: var(--color-text);">"Ask anything about "(brand)" or try one of the examples below."</p>
            <div id="example-chips" class="flex flex-wrap gap-2 mb-6">
                for ex in &ctx.landing.examples {
                    <button class="example-chip px-1.5 py-0.5 text-[10px] leading-none rounded-sm cursor-pointer transition" style="background: var(--color-bg-weak); color: var(--color-text-weak);" data-query=(&ex.query)>(&ex.query)</button>
                }
            </div>
            <div id="chat-messages">
                <div id="fallback-hint" class="flex flex-col items-center justify-center py-12 gap-3">
                    <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-weak)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="opacity: 0.5;"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
                    <p class="text-center leading-relaxed" style="color: var(--color-text-weak); font-size: 0.8125rem;">(&ctx.landing.hero.fallback)</p>
                </div>
            </div>
            <div id="typing-indicator" class="hidden" style="padding: 12px 16px 12px 52px; margin-bottom: 10px; position: relative;"></div>
            <div id="chat-bottom-bar" style="position: sticky; bottom: 0; z-index: 10; background: var(--color-bg); padding-top: 12px; margin-top: 8px;">
                <div id="auth-section">
                    <div id="auth-form" class="flex flex-col gap-3 mt-6 p-5 rounded-xl" style="border: 1px solid var(--color-border); background: var(--color-bg);">
                        <div class="flex items-center gap-3 mb-1">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="var(--color-bg-interactive)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"/></svg>
                            <span class="text-sm font-bold" style="color: var(--color-text-strong);">"Welcome! Enter your phone to start"</span>
                        </div>
                        <div class="flex flex-col sm:flex-row gap-2">
                            <select id="country-code" class="px-3 py-2.5 text-sm rounded-lg outline-none transition" style="border: 1.5px solid var(--color-border); background: var(--color-bg-weak); color: var(--color-text-strong); font-family: var(--font-mono); min-width: 5rem;">
                                <option value="256">"🇺🇬 +256"</option>
                                <option value="1">"🇺🇸 +1"</option>
                                <option value="44">"🇬🇧 +44"</option>
                                <option value="254">"🇰🇪 +254"</option>
                                <option value="255">"🇹🇿 +255"</option>
                                <option value="250">"🇷🇼 +250"</option>
                                <option value="257">"🇧🇮 +257"</option>
                                <option value="243">"🇨🇩 +243"</option>
                                <option value="211">"🇸🇸 +211"</option>
                                <option value="256" disabled="">"──────────"</option>
                                <option value="91">"🇮🇳 +91"</option>
                                <option value="86">"🇨🇳 +86"</option>
                                <option value="81">"🇯🇵 +81"</option>
                                <option value="233">"🇬🇭 +233"</option>
                                <option value="234">"🇳🇬 +234"</option>
                                <option value="27">"🇿🇦 +27"</option>
                                <option value="33">"🇫🇷 +33"</option>
                                <option value="49">"🇩🇪 +49"</option>
                                <option value="61">"🇦🇺 +61"</option>
                            </select>
                            <input id="phone-input" type="tel" placeholder="Phone number (e.g. 757337153)" class="flex-1 px-4 py-2.5 text-sm rounded-lg outline-none transition" style="border: 1.5px solid var(--color-border); background: var(--color-bg-weak); color: var(--color-text-strong); font-family: var(--font-mono);"/>
                        </div>
                        <input id="name-input" type="text" placeholder="Name (optional)" class="w-full px-4 py-2.5 text-sm rounded-lg outline-none transition" style="border: 1.5px solid var(--color-border); background: var(--color-bg-weak); color: var(--color-text-strong); font-family: var(--font-mono);"/>
                        <button id="register-btn" class="w-full px-6 py-2.5 text-sm font-medium rounded-lg cursor-pointer transition" style="background: var(--color-bg-interactive); color: var(--color-text-inverted); border: none;">"Get Started →"</button>
                        <p id="auth-error" class="text-xs hidden" style="color: #ff3b30; text-align: center;"></p>
                    </div>
                </div>
                <div id="user-badge" class="hidden flex items-center gap-2 mt-4 px-3 py-1.5 rounded-lg" style="border: 1px solid var(--color-border-weak); background: var(--color-bg-weak);">
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-weak)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"/></svg>
                    <span id="badge-display" class="text-xs flex-1" style="color: var(--color-text-strong);"></span>
                    <button id="logout-btn" class="text-xs px-2 py-0.5 rounded cursor-pointer transition" style="color: var(--color-text-weak); border: 1px solid var(--color-border-weak); background: transparent;">"Logout"</button>
                </div>
                <form id="chat-form" style="padding-top: 8px;">
                    <div style="position: relative;">
                        <textarea id="chat-input" placeholder="Type your message here..." rows="2" class="w-full px-5 py-3.5 pr-12 text-sm rounded-xl resize-none outline-none transition leading-relaxed" style="min-height: 52px; max-height: 160px; border: 1.5px solid var(--color-border); background: var(--color-bg-weak); color: var(--color-text-strong); font-family: var(--font-mono);"></textarea>
                        <button id="send-btn" class="absolute bottom-2 right-2 z-10 flex items-center justify-center cursor-pointer transition rounded-lg" style="width: 32px; height: 32px; border: none; background: var(--color-bg-interactive); color: var(--color-text-inverted);">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>
                        </button>
                        <button id="cancel-btn" class="absolute bottom-2 right-2 z-10 hidden items-center justify-center cursor-pointer transition rounded-lg" style="width: 32px; height: 32px; border: none; background: var(--color-status-error); color: #fff;">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/></svg>
                        </button>
                    </div>
                </form>
            </div>
            <div class="pt-6 pb-2">
                <h1 style="font-size: 1.25rem; font-weight: 700; margin-bottom: 0.25rem; color: var(--color-text-strong);">(&ctx.landing.hero.title)</h1>
                <p style="font-size: 0.875rem; width: 100%; line-height: 1.75; color: var(--color-text);">(&ctx.landing.hero.description)</p>
            </div>
            <section id="faq-below" style="border-top: 1px solid var(--color-border-weak); margin-top: 24px;">
                <h3 class="text-sm font-bold mb-6" style="color: var(--color-text-strong); padding-top: 24px;">"FAQ"</h3>
                <ul class="list-none m-0 p-0">
                    for item in &ctx.landing.faq {
                        <li style="border-top: 1px solid var(--color-border-weak); padding: 12px 0;">
                            <details class="faq-item group">
                                <summary style="display: flex; align-items: center; gap: 8px; font-size: 0.875rem; font-weight: 500; cursor: pointer; color: var(--color-text-strong); list-style: none;">
                                    <span class="faq-icon" style="width: 16px; text-align: center; flex-shrink: 0; color: var(--color-text-weak);">"[+]"</span>
                                    <span>(&item.q)</span>
                                </summary>
                                <div class="ml-9 pb-2 text-sm leading-relaxed" style="color: var(--color-text);">(&item.a)</div>
                            </details>
                        </li>
                    }
                </ul>
            </section>
        </section>
        owner_login()
        <script>{ (Unescaped::new_unchecked(r#"
        document.querySelectorAll('.faq-item').forEach(function(details) {
            details.addEventListener('toggle', function() {
                var icon = this.querySelector('.faq-icon');
                if (icon) icon.textContent = this.open ? '[-]' : '[+]';
            });
        });
        "#)) }</script>
    }
}
