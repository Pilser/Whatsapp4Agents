use topcoat::{
    Result,
    context::Cx,
    icon::{IconData, icon, iconify::iconify_icon},
    view::{Attributes, View, attributes, class, component, view},
};

const SELECT: &str = "h-9 w-full appearance-none items-center rounded-lg border border-border \
    bg-background pr-8 pl-3 text-left text-sm shadow-xs transition-colors outline-none \
    focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
    focus-visible:ring-offset-background disabled:pointer-events-none";

const PICKER: &str = "[&::picker(select)]:[appearance:base-select] \
    [&::picker(select)]:mt-1 [&::picker(select)]:rounded-lg \
    [&::picker(select)]:border [&::picker(select)]:border-border \
    [&::picker(select)]:bg-background [&::picker(select)]:p-1 \
    [&::picker(select)]:text-foreground [&::picker(select)]:shadow-sm \
    [&::picker-icon]:hidden \
    [&_option]:flex [&_option]:items-center [&_option]:gap-2 [&_option]:rounded-md \
    [&_option]:px-2 [&_option]:py-1.5 [&_option]:text-sm [&_option]:outline-none \
    [&_option:hover]:bg-foreground/5 [&_option:focus]:bg-foreground/5 \
    [&_option:checked]:font-medium \
    [&_option::checkmark]:order-1 [&_option::checkmark]:ml-auto \
    [&_option::checkmark]:size-4 [&_option::checkmark]:shrink-0 \
    [&_option::checkmark]:content-[''] [&_option::checkmark]:bg-muted-foreground \
    [&_option::checkmark]:[mask-size:100%_100%] \
    [&_option::checkmark]:[mask-image:var(--select-checkmark)]";

const CHECKMARK: IconData = iconify_icon!("feather:check");

fn checkmark_style(cx: &Cx) -> String {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{}">{}</svg>"#,
        CHECKMARK.view_box(),
        CHECKMARK.into_body().render(cx),
    );
    let mut style = String::from(r#"--select-checkmark: url("data:image/svg+xml,"#);
    for char in svg.chars() {
        match char {
            '%' => style.push_str("%25"),
            '"' => style.push_str("%22"),
            '#' => style.push_str("%23"),
            _ => style.push(char),
        }
    }
    style.push_str(r#"")"#);
    style
}

#[component]
pub async fn select(cx: &Cx, #[default] mut attrs: Attributes, #[default] child: View) -> Result {
    view! {
        <span
            class=(class!(
                "relative block has-[:disabled]:opacity-50 \
                 [&>select]:[appearance:base-select] \
                 [&:has(select:open)>svg]:rotate-180",
                attrs.remove("class"),
            ))
            style=(checkmark_style(cx))
        >
            <select class=(class!(SELECT, PICKER)) (attrs)>(child)</select>
            icon(
                data: iconify_icon!("feather:chevron-down"),
                attrs: attributes! {
                    class="pointer-events-none absolute top-1/2 right-3 size-4 \
                        -translate-y-1/2 text-muted-foreground transition-transform"
                }
            )
        </span>
    }
}
