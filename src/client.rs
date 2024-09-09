use egui::{Context, Id};
use ehttp::Request;
use serde::{Deserialize, Serialize};

use crate::{export, loading::Loading, notifications::Notifications};

pub const API_BASE: &str = "http://localhost:1337/api";

#[derive(Clone, Serialize, Deserialize)]
pub struct Client {
    user_data: Option<UserData>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UserData {
    pub email: String,
    pub id: usize,
    pub session: Session,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    id: String,
}

// ----------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ProjectEntry {
    pub id: i32,
    pub name: String,
    pub is_public: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct Project {
    pub id: i32,
    pub user_id: i32,
    pub name: String,
    /// We store the data as JSON, but we don't need to deserialize it here.
    pub data: serde_json::Value,
    pub is_public: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ----------------------------------------------------------------------------------

impl Client {
    pub fn new() -> Self {
        Self { user_data: None }
    }

    fn load(ctx: &Context) -> Self {
        ctx.data_mut(|d| d.get_persisted(Id::NULL))
            .unwrap_or(Self::new())
    }

    fn store(self, ctx: &Context) {
        ctx.data_mut(|d| d.insert_persisted(Id::NULL, self))
    }

    fn modify(ctx: &Context, f: impl FnOnce(&mut Self)) {
        let mut client = Self::load(ctx);
        f(&mut client);
        client.store(ctx);
    }

    pub fn user_data(ctx: &Context) -> Option<UserData> {
        Self::load(ctx).user_data
    }

    pub fn signup(
        ctx: &Context,
        email: &str,
        password: &str,
        on_done: impl 'static + Send + Clone + FnOnce(Option<Session>),
    ) {
        let body = serde_json::json!({
            "email": email,
            "password": password,
        });

        #[derive(Deserialize)]
        struct Response {
            user_id: usize,
            session_id: String,
        }

        let ctx2 = ctx.clone();
        let email = email.to_string();
        let on_done2 = on_done.clone();

        Self::post_json(
            ctx,
            "user/create",
            body,
            || {},
            move |response: Response| {
                let session = Session {
                    id: response.session_id,
                };
                Self::modify(&ctx2, |slf| {
                    slf.user_data = Some(UserData {
                        email,
                        id: response.user_id,
                        session: session.clone(),
                    });
                });
                on_done(Some(session))
            },
            move || on_done2(None),
        );
    }

    /// Handles errors and notifications.
    pub fn login(
        ctx: &Context,
        email: &str,
        password: &str,
        on_done: impl 'static + Send + Clone + FnOnce(Option<Session>),
    ) {
        let body = serde_json::json!({
            "email": email,
            "password": password,
        });

        #[derive(Deserialize)]
        struct Response {
            user_id: usize,
            session_id: String,
        }

        let ctx2 = ctx.clone();
        let email = email.to_string();
        let on_done2 = on_done.clone();

        Self::post_json(
            ctx,
            "user/login",
            body,
            || {},
            move |response: Response| {
                let session = Session {
                    id: response.session_id,
                };
                Self::modify(&ctx2, |slf| {
                    slf.user_data = Some(UserData {
                        email,
                        id: response.user_id,
                        session: session.clone(),
                    });
                });
                on_done(Some(session))
            },
            move || on_done2(None),
        );
    }

    pub fn logout(ctx: &Context, on_done: impl 'static + Send + FnOnce()) {
        let ctx2 = ctx.clone();
        Self::post_json::<(), ()>(
            ctx,
            "user/logout",
            (),
            move || {
                Self::modify(&ctx2, |slf| {
                    slf.user_data = None;
                });
                on_done();
            },
            |_| {},
            || {},
        );
    }

    // ----------------------------------------------------------------------------------

    pub fn create_project(
        ctx: &Context,
        name: &str,
        data: export::Project,
        on_success: impl 'static + Send + FnOnce(i32),
    ) {
        #[derive(Deserialize)]
        struct ProjectId {
            project_id: i32,
        }

        let payload = serde_json::json!({
            "name": name,
            "data": data.export_json(),
            "is_public": false,
        });

        Self::post_json::<serde_json::Value, ProjectId>(
            ctx,
            "project/create",
            payload,
            || {},
            |p| on_success(p.project_id),
            || {},
        );
    }

    pub fn list_projects(
        ctx: &Context,
        on_success: impl 'static + Send + FnOnce(Vec<ProjectEntry>),
    ) {
        Self::get_json(ctx, "projects", || {}, on_success, || {});
    }

    pub fn load_project(
        ctx: &Context,
        project_id: i32,
        on_success: impl 'static + Send + FnOnce(Project),
    ) {
        Self::get_json(
            ctx,
            format!("project/{project_id}").as_str(),
            || {},
            on_success,
            || {},
        );
    }

    pub fn set_project_public(
        ctx: &Context,
        project_id: i32,
        is_public: bool,
        on_done: impl 'static + Send + FnOnce(),
    ) {
        Self::post_json::<bool, ()>(
            ctx,
            format!("project/{project_id}/public").as_str(),
            is_public,
            on_done,
            |_| {},
            || {},
        );
    }

    pub fn set_project_data(
        ctx: &Context,
        project_id: i32,
        project: export::Project,
        on_done: impl 'static + Send + FnOnce(),
    ) {
        Self::post_json::<serde_json::Value, ()>(
            ctx,
            format!("project/{project_id}/data").as_str(),
            project.export_json(),
            on_done,
            |_| {},
            || {},
        );
    }

    pub fn set_project_name(
        ctx: &Context,
        project_id: i32,
        name: &str,
        on_done: impl 'static + Send + FnOnce(),
    ) {
        Self::post_json::<String, ()>(
            ctx,
            format!("project/{project_id}/name").as_str(),
            name.to_string(),
            on_done,
            |_| {},
            || {},
        );
    }

    // ----------------------------------------------------------------------------------

    pub fn post_json<I: Serialize, O: for<'de> Deserialize<'de>>(
        ctx: &Context,
        path: &str,
        body: I,
        on_done: impl 'static + Send + FnOnce(),
        on_success: impl 'static + Send + FnOnce(O),
        on_error: impl 'static + Send + FnOnce(),
    ) {
        Self::fetch_json(
            ctx,
            Request::json(format!("{API_BASE}/{path}"), &body).unwrap(),
            on_done,
            on_success,
            on_error,
        );
    }

    pub fn get_json<O: for<'de> Deserialize<'de>>(
        ctx: &Context,
        path: &str,
        on_done: impl 'static + Send + FnOnce(),
        on_success: impl 'static + Send + FnOnce(O),
        on_error: impl 'static + Send + FnOnce(),
    ) {
        Self::fetch_json(
            ctx,
            Request::get(format!("{API_BASE}/{path}")),
            on_done,
            on_success,
            on_error,
        );
    }

    /// Automatically adds session header if user is logged in.
    fn fetch_json<O: for<'de> Deserialize<'de>>(
        ctx: &Context,
        mut request: Request,
        on_done: impl 'static + Send + FnOnce(),
        on_success: impl 'static + Send + FnOnce(O),
        on_error: impl 'static + Send + FnOnce(),
    ) {
        Loading::start_loading(ctx);

        let ctx2 = ctx.clone();
        let error = move |err: &str| {
            Notifications::error(&ctx2, "Api request failed.", Some(err));
            on_error();
        };

        if let Some(user_data) = Self::load(ctx).user_data {
            request.headers.insert("Session", user_data.session.id);
        }

        let ctx = ctx.clone();
        ehttp::fetch(request, move |response| {
            on_done();
            Loading::loading_done(&ctx);
            match response {
                Ok(response) => {
                    if response.status == 200 {
                        if let Some(text) = response.text() {
                            match serde_json::from_str(text) {
                                Ok(json) => on_success(json),
                                Err(err) => error(&format!("Could not decode Api response: {err}")),
                            }
                        } else {
                            error("Response was empty.");
                        }
                    } else {
                        error(response.text().unwrap_or_default())
                    }
                }
                Err(err) => error(&err),
            }
        });
    }
}