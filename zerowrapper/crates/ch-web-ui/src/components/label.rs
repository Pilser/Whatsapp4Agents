use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

const LABEL: &str = "flex items-center gap-2 text-sm leading-none font-medium select-none \
    peer-disabled:pointer-events-none peer-disabled:opacity-50 \
    has-[:disabled]:pointer-events-none has-[:disabled]:opacity-50";

#[component]
pub async fn label(#[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! { <label class=(class!(LABEL, attrs.remove("class"))) (attrs)>(child)</label> }
}
