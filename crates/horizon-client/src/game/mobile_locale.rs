//! Original bootstrap strings available before Minecraft resources are downloaded.
//! Setup selects the saved Minecraft language or Android locale before drawing.

pub(super) fn tr(key: &str) -> String {
    text(&horizon_client::lang::code(), key).to_owned()
}

fn text(code: &str, key: &str) -> &'static str {
    let (en, es, pt) = match key {
        "installation" => (
            "Horizon · Installation",
            "Horizon · Instalación",
            "Horizon · Instalação",
        ),
        "retry" => ("Retry", "Reintentar", "Tentar novamente"),
        "installer_failed" => (
            "The installer stopped unexpectedly.",
            "El preparador se cerró inesperadamente.",
            "O instalador parou inesperadamente.",
        ),
        "verify_first" => (
            "Sign in to verify Minecraft Java.",
            "Inicia sesión para verificar Minecraft Java.",
            "Entre para verificar o Minecraft Java.",
        ),
        "title" => ("Microsoft account", "Cuenta Microsoft", "Conta Microsoft"),
        "subtitle" => (
            "Your Minecraft Java profile",
            "Tu perfil de Minecraft Java",
            "Seu perfil do Minecraft Java",
        ),
        "sign_in" => ("Sign in", "Iniciar sesión", "Entrar"),
        "sign_in_microsoft" => (
            "Sign in with Microsoft",
            "Iniciar sesión con Microsoft",
            "Entrar com a Microsoft",
        ),
        "continue" => ("Continue", "Continuar", "Continuar"),
        "cancel" => ("Cancel", "Cancelar", "Cancelar"),
        "back" => ("Back", "Volver", "Voltar"),
        "exit" => ("Exit", "Salir", "Sair"),
        "open_microsoft" => ("Open Microsoft", "Abrir Microsoft", "Abrir Microsoft"),
        "sign_out" => ("Sign out", "Cerrar sesión", "Sair da conta"),
        "storage" => (
            "Could not open private account storage.",
            "No se pudo abrir el almacenamiento privado de la cuenta.",
            "Não foi possível abrir o armazenamento privado da conta.",
        ),
        "unconfigured" => (
            "Sign-in is not available in this build.",
            "El inicio de sesión no está disponible en esta versión.",
            "O login não está disponível nesta versão.",
        ),
        "signed_out" => (
            "Use your Outlook, Hotmail or Microsoft account to continue.",
            "Usa tu cuenta de Outlook, Hotmail o Microsoft para continuar.",
            "Use sua conta Outlook, Hotmail ou Microsoft para continuar.",
        ),
        "starting" => (
            "Connecting to Microsoft...",
            "Conectando con Microsoft...",
            "Conectando à Microsoft...",
        ),
        "refreshing" => (
            "Restoring your session...",
            "Restaurando tu sesión...",
            "Restaurando sua sessão...",
        ),
        "browser" => (
            "Finish signing in on Microsoft's official page, then return to the game.",
            "Completa el acceso en la página oficial de Microsoft y vuelve al juego.",
            "Conclua o login na página oficial da Microsoft e volte ao jogo.",
        ),
        "signed_in" => (
            "Signed in as {}. Your name and skin are active.",
            "Conectado como {}. Tu nick y skin están activos.",
            "Conectado como {}. Seu nome e skin estão ativos.",
        ),
        "code" => ("Code: {}", "Código: {}", "Código: {}"),
        "copied" => (
            "Code copied. Paste it if Microsoft asks.",
            "El código se copió. Pégalo si Microsoft lo pide.",
            "Código copiado. Cole se a Microsoft solicitar.",
        ),
        "code_help" => (
            "If Microsoft asks, enter this code.",
            "Si Microsoft lo pide, escribe este código.",
            "Se a Microsoft solicitar, digite este código.",
        ),
        "privacy" => (
            "Enter your password only on Microsoft's website. Your session will be remembered on this device.",
            "La contraseña se escribe únicamente en Microsoft. Tu sesión se recordará en este dispositivo.",
            "Digite sua senha somente no site da Microsoft. Sua sessão será lembrada neste dispositivo.",
        ),
        "network" => (
            "Could not reach the account service. Try again when connected.",
            "No se pudo contactar con el servicio de cuentas. Intenta de nuevo cuando vuelva la conexión.",
            "Não foi possível acessar o serviço de contas. Tente novamente quando houver conexão.",
        ),
        "approval" => (
            "Minecraft rejected this app registration (HTTP 403). Mojang approval is required for this Client ID.",
            "Minecraft rechazó el registro de esta aplicación (HTTP 403). Se requiere la autorización de Mojang para este Client ID.",
            "Minecraft rejeitou o registro deste aplicativo (HTTP 403). Este Client ID precisa de autorização da Mojang.",
        ),
        "forbidden" => (
            "Minecraft refused access (HTTP 403). Microsoft accepted the account, but Minecraft did not authorize this request.",
            "Minecraft rechazó el acceso (HTTP 403). Microsoft aceptó la cuenta, pero Minecraft no autorizó esta solicitud.",
            "Minecraft recusou o acesso (HTTP 403). A Microsoft aceitou a conta, mas o Minecraft não autorizou esta solicitação.",
        ),
        "expired" => (
            "Your session expired. Sign in again.",
            "Tu sesión venció. Inicia sesión de nuevo.",
            "Sua sessão expirou. Entre novamente.",
        ),
        "unauthorized" => (
            "Minecraft rejected the session (HTTP 401). Sign in again.",
            "Minecraft rechazó la sesión (HTTP 401). Inicia sesión otra vez.",
            "Minecraft rejeitou a sessão (HTTP 401). Entre novamente.",
        ),
        "declined" => (
            "Sign-in was declined in the browser.",
            "Se rechazó el inicio de sesión en el navegador.",
            "O login foi recusado no navegador.",
        ),
        "code_expired" => (
            "The sign-in code expired. Start again.",
            "El código de inicio de sesión venció. Inicia de nuevo.",
            "O código de login expirou. Comece novamente.",
        ),
        "code_invalid" => (
            "The sign-in code was not recognized. Start again.",
            "No se reconoció el código de inicio de sesión. Inicia de nuevo.",
            "O código de login não foi reconhecido. Comece novamente.",
        ),
        "ms_registration" => (
            "Microsoft refused this app's registration (Client ID).",
            "Microsoft rechazó el registro de esta aplicación (Client ID).",
            "A Microsoft recusou o registro deste aplicativo (Client ID).",
        ),
        "failed" => (
            "Sign-in failed. Try again.",
            "No se pudo iniciar sesión. Intenta de nuevo.",
            "Não foi possível entrar. Tente novamente.",
        ),
        "cancelled" => (
            "Sign-in cancelled.",
            "Inicio de sesión cancelado.",
            "Login cancelado.",
        ),
        "xbox_profile" => (
            "This account has no Xbox profile. Sign in at xbox.com to create one, then try again.",
            "Esta cuenta no tiene perfil de Xbox. Créalo iniciando sesión en xbox.com e intenta de nuevo.",
            "Esta conta não tem perfil Xbox. Entre em xbox.com para criar um e tente novamente.",
        ),
        "xbox_region" => (
            "Xbox Live is unavailable in your country or region.",
            "Xbox Live no está disponible en tu país o región.",
            "O Xbox Live não está disponível no seu país ou região.",
        ),
        "xbox_adult" => (
            "This account needs adult verification at xbox.com before playing.",
            "Esta cuenta requiere verificación de adulto en xbox.com antes de jugar.",
            "Esta conta precisa de verificação de adulto em xbox.com antes de jogar.",
        ),
        "xbox_child" => (
            "An adult must add this child account to a Microsoft family first.",
            "Un adulto debe agregar esta cuenta infantil a una familia de Microsoft.",
            "Um adulto precisa adicionar esta conta infantil a uma família Microsoft.",
        ),
        "xbox_failed" => (
            "Xbox Live refused sign-in.",
            "Xbox Live rechazó el inicio de sesión.",
            "O Xbox Live recusou o login.",
        ),
        "rate_limit" => (
            "Too many sign-in attempts. Wait a minute and try again.",
            "Demasiados intentos de inicio de sesión. Espera un minuto e intenta de nuevo.",
            "Muitas tentativas de login. Aguarde um minuto e tente novamente.",
        ),
        "mc_failed" => (
            "Minecraft Services refused sign-in.",
            "Los servicios de Minecraft rechazaron el inicio de sesión.",
            "Os serviços do Minecraft recusaram o login.",
        ),
        "ownership_check" => (
            "Could not verify game ownership. Try again.",
            "No se pudo verificar la propiedad del juego. Intenta de nuevo.",
            "Não foi possível verificar a propriedade do jogo. Tente novamente.",
        ),
        "no_profile" => (
            "This account has no Minecraft profile. Create one at minecraft.net and sign in again.",
            "Esta cuenta no tiene perfil de Minecraft. Créalo en minecraft.net e inicia sesión de nuevo.",
            "Esta conta não tem perfil do Minecraft. Crie um em minecraft.net e entre novamente.",
        ),
        "profile_invalid" => (
            "Could not read a valid Minecraft profile. Sign in again.",
            "No se pudo leer un perfil válido de Minecraft. Inicia sesión de nuevo.",
            "Não foi possível ler um perfil válido do Minecraft. Entre novamente.",
        ),
        "not_owned" => (
            "This account does not own Minecraft: Java Edition.",
            "Esta cuenta no tiene Minecraft: Java Edition.",
            "Esta conta não possui Minecraft: Java Edition.",
        ),
        "profile_changed" => (
            "The Minecraft profile changed. Sign in again.",
            "El perfil de Minecraft cambió. Inicia sesión de nuevo.",
            "O perfil do Minecraft mudou. Entre novamente.",
        ),
        "save_failed" => (
            "Could not save the private session. Sign in again after restarting.",
            "No se pudo guardar la sesión privada. Deberás iniciar sesión al reabrir.",
            "Não foi possível salvar a sessão privada. Entre novamente ao reiniciar.",
        ),
        "sign_out_failed" => (
            "Could not remove this device's private session.",
            "No se pudo eliminar la sesión privada de este dispositivo.",
            "Não foi possível remover a sessão privada deste dispositivo.",
        ),
        "cache_invalid" => (
            "The saved session could not be read safely. Sign in again.",
            "No se pudo leer la sesión guardada de forma segura. Inicia sesión de nuevo.",
            "Não foi possível ler a sessão salva com segurança. Entre novamente.",
        ),
        "browser_failed" => (
            "Could not open Microsoft's sign-in page. Try again.",
            "No se pudo abrir la página de inicio de sesión de Microsoft. Intenta de nuevo.",
            "Não foi possível abrir a página de login da Microsoft. Tente novamente.",
        ),
        _ => ("", "", ""),
    };
    match code.split(['_', '-']).next().unwrap_or("en") {
        "es" => es,
        "pt" => pt,
        _ => en,
    }
}

/// Only known safe messages are rendered. Unexpected service text is never echoed.
pub(super) fn localize_error(error: &str) -> String {
    tr(error_key(error))
}

fn error_key(error: &str) -> &'static str {
    match error {
        "Your Microsoft sign-in expired. Sign in again."
        | "La sesión de Minecraft venció. Iniciá sesión de nuevo." => "expired",
        "Sign-in was declined in the browser." => "declined",
        "The sign-in code expired. Start again." => "code_expired",
        "The sign-in code was not recognised. Start again." => "code_invalid",
        "Microsoft refused this launcher's app registration (client ID)." => "ms_registration",
        "Sign-in cancelled." => "cancelled",
        "This Microsoft account has no Xbox profile yet. Sign in once at xbox.com to create one, then try again." => {
            "xbox_profile"
        }
        "Xbox Live is not available in your country or region." => "xbox_region",
        "This account needs adult verification on xbox.com before it can play." => "xbox_adult",
        "This is a child account: an adult has to add it to a Microsoft family first." => {
            "xbox_child"
        }
        "Xbox Live refused the sign-in." => "xbox_failed",
        "Too many sign-ins in a short time. Wait a minute and try again." => "rate_limit",
        "The Minecraft services refused the sign-in." => "mc_failed",
        "Could not check which games this account owns." => "ownership_check",
        "This account has no Minecraft profile yet. Create one at minecraft.net, then sign in again." => {
            "no_profile"
        }
        "Could not read the Minecraft profile." | "The Minecraft profile is invalid." => {
            "profile_invalid"
        }
        "This account doesn't own Minecraft: Java Edition." => "not_owned",
        "The Microsoft account now has another Minecraft profile. Sign in again."
        | "The Minecraft profile changed. Sign in again." => "profile_changed",
        "No se pudo guardar la sesión privada."
        | "No se pudo guardar la sesión en almacenamiento privado." => "save_failed",
        "No se pudo eliminar la sesión privada de este dispositivo." => "sign_out_failed",
        "No se pudo leer la sesión privada."
        | "La sesión privada no es válida. Iniciá sesión de nuevo."
        | "La sesión guardada no tiene permisos privados. Iniciá sesión de nuevo."
        | "La sesión privada está dañada. Iniciá sesión de nuevo." => "cache_invalid",
        "Android no dio acceso al almacenamiento privado."
        | "No se pudo abrir el almacenamiento privado de Android."
        | "Las cuentas requieren el almacenamiento interno privado de Android."
        | "El directorio privado de cuentas no es seguro."
        | "No se pudo crear el almacenamiento privado de cuentas."
        | "No se pudo proteger el almacenamiento de cuentas." => "storage",
        "El identificador de aplicación debe ser un GUID público válido." => "unconfigured",
        "Microsoft devolvió una dirección de verificación no reconocida." => "browser_failed",
        _ if error == text("es", "approval") => "approval",
        _ if error == text("es", "forbidden") => "forbidden",
        _ if error == text("es", "unauthorized") => "unauthorized",
        _ if error.starts_with("No se pudo contactar con ") => "network",
        _ if error.starts_with("Xbox Live refused the sign-in (error ") => "xbox_failed",
        _ if error.starts_with("Conectado como ")
            && error.contains("No se pudo guardar la sesión") =>
        {
            "save_failed"
        }
        _ if error.contains("docs/microsoft-account.md") => "unconfigured",
        _ => "failed",
    }
}

/// Normalize Android BCP-47 tags to Minecraft's resource language codes.
/// Regional variants with no matching game translation fall back to the language's base pack.
pub(crate) fn minecraft_locale(tag: &str) -> String {
    let normalized = tag.to_ascii_lowercase().replace('-', "_");
    let mut parts = normalized.split('_');
    let language = parts.next().unwrap_or("en");
    let region = parts.find(|part| part.len() == 2).unwrap_or("");
    match language {
        "es" => format!(
            "es_{}",
            match region {
                "ar" | "cl" | "ec" | "mx" | "uy" | "ve" => region,
                _ => "es",
            }
        ),
        "pt" => if region == "br" { "pt_br" } else { "pt_pt" }.into(),
        "en" => format!(
            "en_{}",
            match region {
                "au" | "ca" | "gb" | "nz" => region,
                _ => "us",
            }
        ),
        "de" => "de_de".into(),
        "fr" => if region == "ca" { "fr_ca" } else { "fr_fr" }.into(),
        "it" => "it_it".into(),
        "ja" => "ja_jp".into(),
        "ko" => "ko_kr".into(),
        "ru" => "ru_ru".into(),
        "zh" => if matches!(region, "tw" | "hk") {
            "zh_tw"
        } else {
            "zh_cn"
        }
        .into(),
        _ => "en_us".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn android_locale_maps_to_game_language() {
        assert_eq!(super::minecraft_locale("es-AR"), "es_ar");
        assert_eq!(super::minecraft_locale("es-CO"), "es_es");
        assert_eq!(super::minecraft_locale("pt-BR"), "pt_br");
        assert_eq!(super::minecraft_locale("en-GB"), "en_gb");
        assert_eq!(super::minecraft_locale("zh-Hant-TW"), "zh_tw");
        assert_eq!(super::minecraft_locale("unknown"), "en_us");
    }
    #[test]
    fn language_variants_and_consistent_fallback() {
        assert_eq!(text("es_ar", "sign_in"), "Iniciar sesión");
        assert_eq!(text("pt-BR", "exit"), "Sair");
        for key in [
            "title", "subtitle", "privacy", "approval", "sign_in", "cancel",
        ] {
            assert_eq!(text("ja_jp", key), text("en_us", key));
        }
    }
    #[test]
    fn errors_preserve_cause_without_echoing_server_text() {
        assert_eq!(
            error_key(
                "No se pudo contactar con Microsoft. Intenta de nuevo cuando vuelva la conexión."
            ),
            "network"
        );
        assert_eq!(error_key(text("es", "approval")), "approval");
        assert_eq!(error_key(text("es", "forbidden")), "forbidden");
        assert_eq!(error_key("<html>unexpected secret=123</html>"), "failed");
    }
}
