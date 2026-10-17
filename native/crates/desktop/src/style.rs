//! Shared native studio materials and typography, based on the approved concept.
use iced::widget::{button, container};
use iced::{Background, Border, Color, Shadow, Theme, Vector, gradient};
pub const BASE: Color = iced::color!(0x080d11);
pub const EDGE: Color = iced::color!(0x283640);
pub const TEXT: Color = iced::color!(0xe3eaf0);
pub const MUTED: Color = iced::color!(0x8c9ca9);
pub const BLUE: Color = iced::color!(0x168bff);
pub const AMBER: Color = iced::color!(0xeab64b);
pub fn theme() -> Theme {
    Theme::custom(
        "Axe Studio",
        iced::theme::Palette {
            background: BASE,
            text: TEXT,
            primary: BLUE,
            success: iced::color!(0x36cb8a),
            warning: AMBER,
            danger: iced::color!(0xef6470),
        },
    )
}
pub fn gradient(top: Color, bottom: Color) -> Background {
    Background::Gradient(
        gradient::Linear::new(std::f32::consts::FRAC_PI_2)
            .add_stop(0., top)
            .add_stop(1., bottom)
            .into(),
    )
}
pub fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(gradient(iced::color!(0x111a21), iced::color!(0x0c1217))),
        border: Border {
            color: EDGE,
            width: 1.,
            radius: 7.into(),
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.35),
            offset: Vector::new(0., 3.),
            blur_radius: 8.,
        },
        ..Default::default()
    }
}
pub fn header(_: &Theme) -> container::Style {
    container::Style {
        background: Some(gradient(iced::color!(0x1b252e), iced::color!(0x121b22))),
        border: Border {
            color: EDGE,
            width: 1.,
            radius: 6.into(),
        },
        ..Default::default()
    }
}
pub fn inset(_: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::color!(0x0a1117).into()),
        border: Border {
            color: EDGE,
            width: 1.,
            radius: 5.into(),
        },
        ..Default::default()
    }
}
pub fn selected(_: &Theme) -> container::Style {
    container::Style {
        background: Some(gradient(iced::color!(0x154d85), iced::color!(0x12355b))),
        border: Border {
            color: iced::color!(0x297dc3),
            width: 1.,
            radius: 5.into(),
        },
        ..Default::default()
    }
}
pub fn neutral(_: &Theme, status: button::Status) -> button::Style {
    let disabled = status == button::Status::Disabled;
    let (top, bottom) = if status == button::Status::Hovered {
        (iced::color!(0x2b3c4a), iced::color!(0x1c2b37))
    } else {
        (iced::color!(0x202d38), iced::color!(0x141e27))
    };
    button::Style {
        background: Some(gradient(top, bottom)),
        text_color: if disabled {
            MUTED.scale_alpha(0.75)
        } else {
            TEXT
        },
        border: Border {
            color: if disabled {
                EDGE.scale_alpha(0.5)
            } else {
                iced::color!(0x364958)
            },
            width: 1.,
            radius: 5.into(),
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.22),
            offset: Vector::new(0., 1.),
            blur_radius: 2.,
        },
        ..Default::default()
    }
}
pub fn primary(t: &Theme, status: button::Status) -> button::Style {
    let mut s = neutral(t, status);
    if status != button::Status::Disabled {
        s.background = Some(gradient(
            if status == button::Status::Hovered {
                iced::color!(0x339dff)
            } else {
                BLUE
            },
            iced::color!(0x0661d1),
        ));
        s.border.color = iced::color!(0x409fff);
    }
    s
}
