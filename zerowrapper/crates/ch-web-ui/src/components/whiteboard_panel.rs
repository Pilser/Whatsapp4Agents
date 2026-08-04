use topcoat::Result;
use topcoat::icon::{icon, iconify};
use topcoat::view::{view, component};

const CHEVRON_DOWN: topcoat::icon::IconData = iconify::iconify_icon!("feather:chevron-down");
const CHEVRON_UP: topcoat::icon::IconData = iconify::iconify_icon!("feather:chevron-up");

#[component]
pub async fn whiteboard_panel() -> Result {
    view! {
        <div
            id="whiteboard-panel"
            class="whiteboard-panel wb-closed"
            style="background: var(--color-bg); border-bottom: 1px solid var(--color-border-weak); box-shadow: 0 4px 24px rgba(0,0,0,0.25);"
        >
            <div id="whiteboard-body" class="flex-1 overflow-y-auto px-4 py-2"></div>
            <div id="whiteboard-pins" class="grid grid-cols-1 sm:grid-cols-2 gap-2 px-4 pb-4"></div>
        </div>
        <button
            id="whiteboard-down"
            type="button"
            class="whiteboard-arrow"
            aria-label="Expand whiteboard"
            title="Expand whiteboard"
        >
            icon(data: CHEVRON_DOWN, size: 18, label: "Expand whiteboard")
        </button>
        <button
            id="whiteboard-up"
            type="button"
            class="whiteboard-arrow"
            aria-label="Shrink whiteboard"
            title="Shrink whiteboard"
        >
            icon(data: CHEVRON_UP, size: 18, label: "Shrink whiteboard")
        </button>
    }
}
