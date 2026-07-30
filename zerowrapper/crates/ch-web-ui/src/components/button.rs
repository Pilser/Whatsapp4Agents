use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Outline,
    Ghost,
    Destructive,
}

impl ButtonVariant {
    fn classes(self) -> &'static str {
        match self {
            Self::Primary => {
                "border-transparent bg-primary text-primary-foreground shadow-xs \
                 hover:bg-primary/90 active:bg-primary/80"
            }
            Self::Secondary => {
                "border-transparent bg-foreground/5 text-foreground shadow-xs \
                 hover:bg-foreground/10 active:bg-foreground/15"
            }
            Self::Outline => {
                "border-border text-foreground shadow-xs hover:bg-foreground/5 \
                 active:bg-foreground/10"
            }
            Self::Ghost => {
                "border-transparent text-foreground hover:bg-foreground/5 active:bg-foreground/10"
            }
            Self::Destructive => {
                "border-transparent bg-destructive text-destructive-foreground shadow-xs \
                 hover:bg-destructive/90 active:bg-destructive/80"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
    Lg,
    Icon,
}

impl ButtonSize {
    fn classes(self) -> &'static str {
        match self {
            Self::Sm => "h-8 gap-1.5 rounded-md px-3 text-xs",
            Self::Md => "h-9 gap-2 rounded-lg px-4 text-sm",
            Self::Lg => "h-10 gap-2 rounded-lg px-5 text-base",
            Self::Icon => "size-9 rounded-lg text-base",
        }
    }
}

const BASE: &str = "inline-flex shrink-0 items-center justify-center border \
    font-medium whitespace-nowrap transition-colors outline-none select-none \
    focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
    focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50";

#[must_use]
pub fn button_variants(variant: ButtonVariant, size: ButtonSize) -> String {
    format!("{BASE} {} {}", variant.classes(), size.classes())
}

#[component]
pub async fn button(
    #[default] variant: ButtonVariant,
    #[default] size: ButtonSize,
    #[default] mut attrs: Attributes,
    #[default] child: View,
) -> Result {
    view! {
        <button
            class=(class!(
                BASE,
                variant.classes(),
                size.classes(),
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </button>
    }
}
