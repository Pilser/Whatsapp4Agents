use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

const CARD: &str = "flex flex-col gap-5 rounded-xl border border-border bg-background py-6 \
    text-foreground shadow-sm";

#[component]
pub async fn card(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! { <div class=(class!(CARD, attrs.remove("class"))) (attrs)>(child)</div> }
}

#[component]
pub async fn card_header(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! {
        <div
            class=(class!("flex flex-col gap-1.5 px-6", attrs.remove("class")))
            (attrs)
        >
            (child)
        </div>
    }
}

#[component]
pub async fn card_title(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! {
        <h3 class=(class!("leading-none font-semibold", attrs.remove("class"))) (attrs)>
            (child)
        </h3>
    }
}

#[component]
pub async fn card_description(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! {
        <p
            class=(class!("text-sm text-muted-foreground", attrs.remove("class")))
            (attrs)
        >
            (child)
        </p>
    }
}

#[component]
pub async fn card_content(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! { <div class=(class!("px-6", attrs.remove("class"))) (attrs)>(child)</div> }
}

#[component]
pub async fn card_footer(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! {
        <div
            class=(class!("flex items-center gap-2 px-6", attrs.remove("class")))
            (attrs)
        >
            (child)
        </div>
    }
}
