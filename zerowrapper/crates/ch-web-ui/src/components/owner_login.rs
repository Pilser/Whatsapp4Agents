use topcoat::view::{view, component};

#[component]
pub async fn owner_login() -> topcoat::Result {
    view! {
        <div id="owner-login-overlay" class="hidden fixed inset-0 z-50 flex items-center justify-center" style="background: rgba(0,0,0,0.6);">
            <div class="w-full max-w-sm mx-4 p-6 rounded-xl" style="background: var(--color-bg); border: 1px solid var(--color-border);">
                <div class="flex items-center justify-between mb-4">
                    <span id="owner-login-title" class="text-sm font-bold" style="color: var(--color-text-strong);">"Owner / Admin Login"</span>
                    <button id="owner-login-close" class="cursor-pointer" style="background: none; border: none; color: var(--color-text-weak); font-size: 1.25rem;">"×"</button>
                </div>

                <div id="owner-login-form">
                    <div class="flex flex-col gap-3">
                        <input id="owner-phone-input" type="tel" placeholder="Phone number" class="w-full px-4 py-2.5 text-sm rounded-lg outline-none transition" style="border: 1.5px solid var(--color-border); background: var(--color-bg-weak); color: var(--color-text-strong); font-family: var(--font-mono);" />
                        <input id="owner-password-input" type="password" placeholder="Password" class="w-full px-4 py-2.5 text-sm rounded-lg outline-none transition" style="border: 1.5px solid var(--color-border); background: var(--color-bg-weak); color: var(--color-text-strong); font-family: var(--font-mono);" />
                        <button id="owner-login-btn" class="w-full px-6 py-2.5 text-sm font-medium rounded-lg cursor-pointer transition" style="background: var(--color-bg-interactive); color: var(--color-text-inverted); border: none;">"Login"</button>
                        <p id="owner-login-error" class="text-xs hidden" style="color: #ff3b30; text-align: center;"></p>
                    </div>
                    <p class="text-xs text-center mt-4" style="color: var(--color-text-weak);">
                        "Don't have your own AI agent?"
                        <button id="owner-contact-link" class="cursor-pointer underline" style="background: none; border: none; color: var(--color-bg-interactive); font-family: inherit; font-size: inherit;">"Contact here"</button>
                    </p>
                </div>

                <div id="owner-contact-info" class="hidden flex flex-col gap-4">
                    <p class="text-sm leading-relaxed" style="color: var(--color-text);">
                        "The future is already here - it's just not evenly distributed."
                        <br/><br/>
                        "I can build and set up cutting-edge AI agents for your business. Whether you need automation, intelligent chatbots, or custom AI solutions, I bring the latest technology to your fingertips."
                    </p>
                    <div class="flex flex-col gap-2 p-3 rounded-lg" style="background: var(--color-bg-weak); border: 1px solid var(--color-border-weak);">
                        <span class="text-xs font-bold" style="color: var(--color-text-strong);">"Contact Information"</span>
                        <a href="mailto:info@pilserlabs.com" class="text-xs no-underline" style="color: var(--color-bg-interactive);">"info@pilserlabs.com"</a>
                        <a href="tel:+256755304241" class="text-xs no-underline" style="color: var(--color-bg-interactive);">"+256 755 304 241"</a>
                    </div>
                    <button id="owner-contact-back" class="text-xs px-3 py-1.5 rounded cursor-pointer transition self-start" style="color: var(--color-text-weak); border: 1px solid var(--color-border-weak); background: transparent;">"Back to login"</button>
                </div>
            </div>

            <style>"
                #owner-phone-input:focus, #owner-password-input:focus { border-color: var(--color-bg-interactive) !important; box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-bg-interactive) 25%, transparent) !important; }
                #owner-phone-input::placeholder, #owner-password-input::placeholder { color: var(--color-text-weak); opacity: 0.7; }
                #owner-login-btn:hover { opacity: 0.85; }
                #owner-login-close:hover { opacity: 0.7; }
            "</style>
        </div>
    }
}
