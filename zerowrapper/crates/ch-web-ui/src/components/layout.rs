use topcoat::Result;
use topcoat::view::{view, Unescaped};
use topcoat::router::layout;
use topcoat::context::{Cx, app_context};

use crate::context::AppContext;
use crate::components::header::site_header;
use crate::components::footer::site_footer;
use crate::components::owner_chat::owner_chat;
use topcoat::asset::asset;

#[layout("/")]
pub async fn root_layout(cx: &Cx, slot: Result) -> Result {
    let ctx: &AppContext = app_context(cx);
    let title = &ctx.settings.site.title;
    let desc = &ctx.settings.site.description;
    let url = &ctx.settings.site.url;
    let favicon = ctx.settings.site.favicon.as_deref().unwrap_or("/favicon.svg");

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(title)</title>
                <meta name="description" content=(desc)>
                <meta property="og:title" content=(title)>
                <meta property="og:description" content=(desc)>
                <meta property="og:url" content=(url)>
                <link rel="icon" type="image/svg+xml" href=(favicon)>
                <link rel="preconnect" href="https://fonts.googleapis.com">
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="">
                <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
                <link rel="stylesheet" href=(topcoat::tailwind::stylesheet!())>
                <script>{ (Unescaped::new_unchecked(r#"(function(){
                    var saved = localStorage.getItem('theme');
                    var mq = window.matchMedia('(prefers-color-scheme: dark)');
                    if (saved) {
                        document.documentElement.setAttribute('data-theme', saved);
                    } else {
                        document.documentElement.setAttribute('data-theme', mq.matches ? 'dark' : 'light');
                        mq.addEventListener('change', function(e) {
                            if (!localStorage.getItem('theme')) {
                                document.documentElement.setAttribute('data-theme', e.matches ? 'dark' : 'light');
                            }
                        });
                    }
                })();"#)) }</script>
            </head>
            <body class="flex flex-col min-h-screen" style="font-family: var(--font-sans); color: var(--color-text); background: var(--color-bg);">
                <canvas id="bg-canvas"></canvas>
                <div id="page-root" class="w-full mx-auto flex flex-col" style="max-width: 67.5rem; min-height: 100dvh; border-left: 1px solid var(--color-border-weak); border-right: 1px solid var(--color-border-weak);">
                    site_header()
                    <main class="flex-1 flex flex-col min-h-0">
                        (slot?)
                    </main>
                    site_footer()
                </div>
                owner_chat()
                <script type="application/json" id="landing-data">(serde_json::to_string(&ctx.landing).unwrap_or_default())</script>
                <script type="application/json" id="settings-data">(serde_json::to_string(&ctx.settings).unwrap_or_default())</script>
                <script type="importmap">{ (Unescaped::new_unchecked(r#"{"imports":{"marked":"https://cdn.jsdelivr.net/npm/marked@18.0.5/lib/marked.esm.js","highlight.js":"https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.11.1/build/es/highlight.min.js"}}"#)) }</script>
                <script type="module" src=(asset!("assets/js/chat/app.js"))></script>
                <script>{ (Unescaped::new_unchecked(r#"
                var canvas = document.getElementById('bg-canvas');
                if (canvas) {
                    var ctx = canvas.getContext('2d');
                    var prefersReduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
                    function resize() {
                        var dpr = window.devicePixelRatio || 1;
                        canvas.width = window.innerWidth * dpr;
                        canvas.height = window.innerHeight * dpr;
                        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
                    }
                    function draw(time) {
                        if (!prefersReduced) requestAnimationFrame(draw);
                        var w = window.innerWidth, h = window.innerHeight;
                        ctx.clearRect(0, 0, w, h);
                        var topSpacing = 12, bottomSpacing = 48;
                        var topSize = 1.5, bottomSize = 4;
                        var topAlpha = 0.28, bottomAlpha = 0.05;
                        var isDark = document.documentElement.getAttribute('data-theme') === 'dark';
                        var dotColor = isDark
                            ? 'hsl(28, 90%, 50%)'
                            : getComputedStyle(document.documentElement).getPropertyValue('--color-bg-strong').trim() || 'hsl(0, 5%, 12%)';
                        ctx.fillStyle = dotColor;
                        var y = 4;
                        while (y < h) {
                            var t = Math.min(y / h, 1);
                            var spacing = topSpacing + (bottomSpacing - topSpacing) * t;
                            var size = topSize + (bottomSize - topSize) * t;
                            var alpha = topAlpha + (bottomAlpha - topAlpha) * t;
                            var wave = Math.sin(y * 0.018 + time * 0.0004) * 2.5;
                            ctx.globalAlpha = alpha;
                            for (var x = -spacing; x < w + spacing; x += spacing) {
                                ctx.beginPath();
                                ctx.arc(x + wave, y, size, 0, Math.PI * 2);
                                ctx.fill();
                            }
                            y += spacing;
                        }
                        ctx.globalAlpha = 1;
                        if (prefersReduced) return;
                    }
                    resize();
                    window.addEventListener('resize', resize);
                    var themeObserver = new MutationObserver(function() { resize(); });
                    themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
                    requestAnimationFrame(draw);
                }
                "#)) }</script>
            </body>
        </html>
    }
}
