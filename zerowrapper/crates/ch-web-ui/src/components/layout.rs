use topcoat::Result;
use topcoat::asset::asset;
use topcoat::view::view;
use topcoat::router::layout;

use crate::components::header::site_header;
use crate::components::footer::site_footer;

#[layout("/")]
pub async fn root_layout(slot: Result) -> Result {
    view! {
        <!DOCTYPE html>
        <html>
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"WAOS — WhatsApp for Agents"</title>
                <link rel="preconnect" href="https://fonts.googleapis.com">
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="">
                <link
                    href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap"
                    rel="stylesheet"
                >
                <link rel="stylesheet" href=(topcoat::tailwind::stylesheet!())>
            </head>
            <body class="min-h-screen flex flex-col">
                site_header()
                <main class="flex-1">
                    (slot?)
                </main>
                site_footer()
                <script type="module" src=(asset!("assets/js/owner.js"))></script>
            </body>
        </html>
    }
}
