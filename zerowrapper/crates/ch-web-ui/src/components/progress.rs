use topcoat::{
    Result,
    view::{Attributes, class, component, view},
};

const PROGRESS: &str = "h-2 w-full appearance-none overflow-hidden rounded-full \
    bg-foreground/10 [&::-webkit-progress-bar]:bg-transparent \
    [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:bg-primary \
    [&::-webkit-progress-value]:transition-all \
    [&::-moz-progress-bar]:rounded-full [&::-moz-progress-bar]:bg-primary";

#[component]
pub async fn progress(
    #[into]
    #[default]
    value: Option<f32>,
    #[default(100.0)]
    max: f32,
    #[default]
    mut attrs: Attributes,
) -> Result {
    view! {
        <progress
            class=(class!(PROGRESS, attrs.remove("class")))
            value=(value)
            max=(max)
            (attrs)
        ></progress>
    }
}
