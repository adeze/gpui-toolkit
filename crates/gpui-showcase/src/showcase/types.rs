use gpui::SharedString;

#[derive(Clone, Debug)]
pub struct User {
    pub id: usize,
    // SharedString: cell renderers clone these every frame, and Rc bumps
    // beat String clones. `id` stays numeric; its text is memoized below.
    pub name: SharedString,
    pub email: SharedString,
    pub role: SharedString,
    pub id_text: SharedString,
}

impl User {
    pub fn new(id: usize, name: &str, email: &str, role: &str) -> Self {
        Self {
            id,
            name: name.into(),
            email: email.into(),
            role: role.into(),
            id_text: id.to_string().into(),
        }
    }
}
