use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BadgeVariant {
    #[default]
    Primary,
    Secondary,
    Outline,
    Destructive,
}

impl BadgeVariant {
    fn classes(self) -> &'static str {
        match self {
            Self::Primary => "border-transparent bg-primary text-primary-foreground",
            Self::Secondary => "border-transparent bg-foreground/5 text-foreground",
            Self::Outline => "border-border text-foreground",
            Self::Destructive => "border-transparent bg-destructive text-destructive-foreground",
        }
    }
}

const BASE: &str = "inline-flex w-fit shrink-0 items-center justify-center gap-1 rounded-md \
    border px-2 py-0.5 text-xs font-medium whitespace-nowrap";

#[must_use]
pub fn badge_variants(variant: BadgeVariant) -> String {
    format!("{BASE} {}", variant.classes())
}

#[component]
pub async fn badge(
    #[default] variant: BadgeVariant,
    #[default] mut attrs: Attributes,
    #[default] child: View,
) -> Result {
    view! {
        <span class=(class!(BASE, variant.classes(), attrs.remove("class"))) (attrs)>
            (child)
        </span>
    }
}
