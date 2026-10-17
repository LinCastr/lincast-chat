//! Marca do dono do grupo. A moderação (remover pessoas, trocar nome e ícone)
//! passou para o grupo e acontece por HTTP.

use iced::Element;
use iced::widget::svg;

use crate::Msg;
use crate::icones;
use crate::tema;

/// Coroa exibida ao lado do nome do dono do grupo.
pub fn badge_dono() -> Element<'static, Msg> {
    let c = tema::cores();
    svg(icones::coroa())
        .width(14)
        .height(14)
        .style(tema::icone_cor(c.destaque))
        .into()
}
