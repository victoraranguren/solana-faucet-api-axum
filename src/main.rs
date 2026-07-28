use axum::{
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_system_interface::instruction::transfer;
use solana_transaction::Transaction;
use std::{env, str::FromStr};

/// Estructura de la solicitud HTTP en formato JSON.
/// Representa el cuerpo enviado por el cliente que solicita Lamports.
#[derive(Deserialize)]
pub struct AirdropRequest {
    /// Dirección pública de Solana (Wallet) que recibirá los Lamports.
    pub pubkey: String,
}

/// Estructura de respuesta HTTP en formato JSON.
/// Retorna el estado del airdrop y la firma de la transacción en la red Solana.
#[derive(Serialize)]
pub struct AirdropResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explorer_url: Option<String>,
}

#[tokio::main]
async fn main() {
    // 1. Cargar variables de entorno desde el archivo .env
    dotenvy::dotenv().ok();

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let bind_addr = format!("0.0.0.0:{}", port);

    // 2. Definir las rutas de la API REST usando Axum
    let app = Router::new()
        .route("/airdrop", post(airdrop))
        .route("/", post(airdrop));

    println!("🚀 Faucet API Server iniciado exitosamente.");
    println!("📡 Escuchando en http://{}", bind_addr);

    // 3. Crear el listener TCP con Tokio y servir la aplicación Axum
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("❌ No se pudo enlazar al puerto especificado");

    axum::serve(listener, app)
        .await
        .expect("❌ Error en la ejecución del servidor HTTP");
}

/// Handler HTTP encargado de procesar la solicitud de airdrop de Lamports.
/// 
/// Pasos internos:
/// 1. Carga la clave privada del Faucet desde la variable de entorno `FAUCET_SECRET_KEY`.
/// 2. Valida la clave pública de la wallet destino enviada en el JSON.
/// 3. Crea una instrucción de transferencia nativa de Lamports en el System Program.
/// 4. Obtiene un blockhash reciente mediante RPC.
/// 5. Firma y transmite la transacción a la red Solana (Devnet).
pub async fn airdrop(
    Json(payload): Json<AirdropRequest>,
) -> (StatusCode, Json<AirdropResponse>) {
    // A. Obtener clave privada de la wallet distribuidora (Faucet)
    let secret_key_str = match env::var("FAUCET_SECRET_KEY") {
        Ok(key) => key,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AirdropResponse {
                    success: false,
                    message: "FAUCET_SECRET_KEY no está configurada en las variables de entorno".to_string(),
                    tx_signature: None,
                    explorer_url: None,
                }),
            );
        }
    };

    // B. Parsear la Keypair del Faucet desde formato Base58
    let faucet_keypair = Keypair::from_base58_string(&secret_key_str);
    let faucet_pubkey = faucet_keypair.pubkey();

    // C. Validar la dirección de la wallet del destinatario
    let recipient_pubkey = match Pubkey::from_str(&payload.pubkey) {
        Ok(pk) => pk,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AirdropResponse {
                    success: false,
                    message: "La clave pública enviada no es una dirección válida de Solana".to_string(),
                    tx_signature: None,
                    explorer_url: None,
                }),
            );
        }
    };

    let rpc_url = env::var("RPC_URL")
        .unwrap_or_else(|_| "https://api.devnet.solana.com".to_string());
    
    let lamports_to_send: u64 = env::var("AIRDROP_AMOUNT_LAMPORTS")
        .ok()
        .and_then(|val| val.parse().ok())
        .unwrap_or(100_000); // 100,000 Lamports por defecto (0.0001 SOL)

    println!("--------------------------------------------------");
    println!("📦 Procesando solicitud de Airdrop:");
    println!("   🔑 Wallet Faucet:       {}", faucet_pubkey);
    println!("   🎯 Wallet Destino:      {}", recipient_pubkey);
    println!("   💰 Cantidad a enviar:   {} Lamports", lamports_to_send);
    println!("--------------------------------------------------");

    // D. Inicializar cliente RPC no bloqueante de Solana
    let rpc_client = RpcClient::new(rpc_url);

    // E. Crear la instrucción de transferencia de Lamports en el System Program
    let transfer_instruction = transfer(
        &faucet_pubkey,
        &recipient_pubkey,
        lamports_to_send,
    );

    // F. Obtener blockhash reciente necesario para validez temporal de la transacción
    let recent_blockhash = match rpc_client.get_latest_blockhash().await {
        Ok(hash) => hash,
        Err(err) => {
            eprintln!("⚠️ Error al obtener blockhash desde el nodo RPC: {:?}", err);
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(AirdropResponse {
                    success: false,
                    message: format!("Error al comunicarse con el nodo RPC de Solana: {}", err),
                    tx_signature: None,
                    explorer_url: None,
                }),
            );
        }
    };

    // G. Crear y firmar la transacción con la clave privada del Faucet
    let transaction = Transaction::new_signed_with_payer(
        &[transfer_instruction],
        Some(&faucet_pubkey),
        &[&faucet_keypair],
        recent_blockhash,
    );

    // H. Enviar y confirmar la transacción en el cluster Solana
    match rpc_client.send_and_confirm_transaction(&transaction).await {
        Ok(signature) => {
            let sig_str = signature.to_string();
            let explorer_link = format!(
                "https://explorer.solana.com/tx/{}?cluster=devnet",
                sig_str
            );
            println!("✅ Transacción confirmada en Devnet!");
            println!("🔗 Explorer URL: {}", explorer_link);

            (
                StatusCode::OK,
                Json(AirdropResponse {
                    success: true,
                    message: format!("Airdrop de {} lamports enviado exitosamente.", lamports_to_send),
                    tx_signature: Some(sig_str),
                    explorer_url: Some(explorer_link),
                }),
            )
        }
        Err(err) => {
            eprintln!("❌ Error al enviar la transacción: {:?}", err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AirdropResponse {
                    success: false,
                    message: format!("Error al procesar la transacción en la red: {:?}", err),
                    tx_signature: None,
                    explorer_url: None,
                }),
            )
        }
    }
}
