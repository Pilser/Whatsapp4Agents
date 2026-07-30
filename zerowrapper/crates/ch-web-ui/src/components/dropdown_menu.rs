use topcoat::{
    Result,
    icon::{icon, iconify::iconify_icon},
    view::{Attributes, View, attributes, class, component, view},
};

#[component]
pub async fn dropdown_menu(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! {
        <details
            class=(class!("group relative inline-block", attrs.remove("class")))
            (attrs)
        >
            (child)
        </details>
    }
}

const TRIGGER: &str = "cursor-pointer list-none [&::-webkit-details-marker]:hidden";

#[component]
pub async fn dropdown_menu_trigger(
    #[default] mut attrs: Attributes,
    #[default] child: View,
) -> Result {
    view! {
        <summary class=(class!(TRIGGER, attrs.remove("class"))) (attrs)>
            (child)
        </summary>
    }
}

const PANEL: &str = "absolute z-50 min-w-40 rounded-lg border border-border bg-background p-1 \
    text-foreground shadow-sm";

#[component]
pub async fn dropdown_menu_content(
    #[default] mut attrs: Attributes,
    #[default] child: View,
) -> Result {
    view! {
        <div
            class=(class!(PANEL, "top-full left-0 mt-1", attrs.remove("class")))
            (attrs)
        >
            (child)
        </div>
    }
}

const ITEM: &str = "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm \
    whitespace-nowrap outline-none hover:bg-foreground/5 focus-visible:bg-foreground/5 \
    active:bg-foreground/10 disabled:pointer-events-none disabled:opacity-50";

#[component]
pub async fn dropdown_menu_item(
    #[default] mut attrs: Attributes,
    #[default] child: View,
) -> Result {
    view! {
        <button class=(class!(ITEM, attrs.remove("class"))) (attrs)>(child)</button>
    }
}

#[component]
pub async fn dropdown_menu_sub(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! {
        <details class=(class!("group/sub relative", attrs.remove("class"))) (attrs)>
            (child)
        </details>
    }
}

#[component]
pub async fn dropdown_menu_sub_trigger(
    #[default] mut attrs: Attributes,
    #[default] child: View,
) -> Result {
    view! {
        <summary
            class=(class!(
                ITEM,
                TRIGGER,
                "group-open/sub:bg-foreground/5",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
            icon(
                data: iconify_icon!("feather:chevron-right"),
                attrs: attributes! { class="ml-auto size-4" }
            )
        </summary>
    }
}

#[component]
pub async fn dropdown_menu_sub_content(
    #[default] mut attrs: Attributes,
    #[default] child: View,
) -> Result {
    view! {
        <div
            class=(class!(PANEL, "top-0 left-full ml-1", attrs.remove("class")))
            (attrs)
        >
            (child)
        </div>
    }
}

#[component]
pub async fn dropdown_menu_label(
    #[default] mut attrs: Attributes,
    #[default] child: View,
) -> Result {
    view! {
        <p
            class=(class!(
                "px-2 py-1.5 text-xs font-medium text-muted-foreground",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </p>
    }
}

#[component]
pub async fn dropdown_menu_separator(#[default] mut attrs: Attributes) -> Result {
    view! {
        <hr class=(class!("-mx-1 my-1 border-border", attrs.remove("class"))) (attrs)>
    }
}
