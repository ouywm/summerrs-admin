pub fn user_room(user_id: i64) -> String {
    format!("user:{user_id}")
}

pub fn role_room(role: &str) -> String {
    format!("role:{role}")
}

pub fn broadcast_room(user_type: &str) -> String {
    format!("all-{user_type}")
}
