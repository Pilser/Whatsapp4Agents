use topcoat::{
    Result,
    icon::{icon, iconify::iconify_icon},
    view::{Attributes, Length, attributes, class, component, view},
};

#[component]
pub async fn spinner(
    #[into]
    #[default(Length::em(1.0))]
    size: Length,
    #[into]
    #[default(String::from("Loading"))]
    label: String,
    #[default]
    mut attrs: Attributes,
) -> Result {
    view! {
        icon(
            data: iconify_icon!("feather:loader"),
            size: size,
            label: label,
            attrs: attributes! {
                class=(class!("animate-spin", attrs.remove("class")))
                (attrs)
            }
        )
    }
}
