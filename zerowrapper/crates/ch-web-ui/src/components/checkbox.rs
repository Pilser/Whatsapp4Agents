use topcoat::{
    Result,
    icon::{icon, iconify::iconify_icon},
    view::{Attributes, attributes, class, component, view},
};

const CHECKBOX: &str = "peer size-4 shrink-0 appearance-none rounded-[4px] border border-border \
    bg-background shadow-xs transition-colors outline-none \
    checked:border-primary checked:bg-primary \
    focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
    focus-visible:ring-offset-background disabled:pointer-events-none";

#[component]
pub async fn checkbox(#[default] mut attrs: Attributes) -> Result {
    view! {
        <span
            class=(class!(
                "relative inline-flex shrink-0 has-[:disabled]:opacity-50",
                attrs.remove("class"),
            ))
        >
            <input type="checkbox" class=(CHECKBOX) (attrs)>
            icon(
                data: iconify_icon!("feather:check"),
                attrs: attributes! {
                    class="pointer-events-none absolute inset-0 m-auto size-3.5 \
                        text-primary-foreground opacity-0 peer-checked:opacity-100"
                }
            )
        </span>
    }
}
