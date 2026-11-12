use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    OpenApi,
};

use crate::{dto, error, routes};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Tulpar API",
        version = "0.1.0",
        description = "Self-hosted communication platform API."
    ),
    paths(
        routes::health::live,
        routes::health::ready,
        routes::health::version,
        routes::auth::register,
        routes::auth::login,
        routes::auth::refresh,
        routes::auth::logout,
        routes::auth::logout_all,
        routes::auth::list_sessions,
        routes::auth::revoke_session,
        routes::auth::revoke_other_sessions,
        routes::auth::verify_email,
        routes::auth::resend_verification,
        routes::auth::forgot_password,
        routes::auth::reset_password,
        routes::auth::change_password,
        routes::auth::reauth,
        routes::auth::device_code,
        routes::auth::approve_device,
        routes::auth::deny_device,
        routes::auth::device_token,
        routes::users::get_me,
        routes::users::update_me,
        routes::users::delete_me,
        routes::users::get_settings,
        routes::users::update_settings,
        routes::users::export_me,
        routes::users::get_user
    ),
    components(schemas(
        error::ProblemDetails,
        error::FieldError,
        dto::SelfUserDto,
        dto::PublicUserDto,
        dto::UserSettingsDto,
        dto::SessionDto,
        dto::UserExport,
        dto::LoginResponse,
        dto::RefreshResponse,
        dto::DeviceTokenResponse,
        dto::RegistrationResponse,
        dto::AccessTokenResponse,
        dto::DeviceCodeResponseDto,
        dto::VersionResponse,
        dto::LivenessResponse,
        dto::HealthResponse,
        dto::HealthChecks,
        dto::RegisterRequest,
        dto::LoginRequest,
        dto::RefreshRequest,
        dto::LogoutRequest,
        dto::EmailRequest,
        dto::ForgotPasswordRequest,
        dto::VerifyEmailRequest,
        dto::ResetPasswordRequest,
        dto::ChangePasswordRequest,
        dto::ReauthRequest,
        dto::DeleteAccountRequest,
        dto::ProfileUpdateRequest,
        dto::SettingsUpdateRequest,
        dto::DeviceCodeRequest,
        dto::DeviceApproveRequest,
        dto::DeviceTokenRequest
    )),
    tags(
        (name = "auth", description = "Registration, authentication, sessions, and device authorization"),
        (name = "users", description = "Profiles and settings"),
        (name = "health", description = "Liveness, readiness, and version")
    )
)]
pub struct ApiDoc;

/// Build the OpenAPI document and add the shared Bearer authentication scheme.
pub fn document() -> utoipa::openapi::OpenApi {
    let mut document = ApiDoc::openapi();
    let components = document.components.get_or_insert_with(Default::default);
    components.add_security_scheme(
        "bearer_auth",
        SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format("JWT")
                .build(),
        ),
    );
    document
}
