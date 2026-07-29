# 🚀 Tutorial: Servidor API REST Faucet para Solana en Rust con Axum

En este tutorial aprenderás a construir desde cero una **API REST en Rust** para enviar **Lamports (Solana Faucet)** a cualquier billetera en la red **Solana Devnet**.

Utilizaremos la arquitectura moderna de **minicrates modulares de Solana v2.x** (`solana-pubkey`, `solana-keypair`, `solana-system-interface`, etc.) junto con el framework web asíncrono **Axum 0.8** y el runtime **Tokio**.

---

## 📑 Tabla de Contenidos

1. [Conceptos Básicos y Arquitectura](#1-conceptos-básicos-y-arquitectura)
2. [Estructura del Proyecto](#2-estructura-del-proyecto)
3. [Requisitos Previos](#3-requisitos-previos)
4. [Paso 1: Configuración de Dependencias (`Cargo.toml`)](#4-paso-1-configuración-de-dependencias-cargotoml)
5. [Paso 2: Variables de Entorno (`.env`)](#5-paso-2-variables-de-entorno-env)
6. [Paso 3: Desarrollo del Servidor API REST (`src/main.rs`)](#6-paso-3-desarrollo-del-servidor-api-rest-srcmainrs)
7. [Paso 4: Compilación y Ejecución](#7-paso-4-compilación-y-ejecución)
8. [Paso 5: Pruebas de la API (curl, Postman, JS)](#8-paso-5-pruebas-de-la-api-curl-postman-js)
9. [Buenas Prácticas y Seguridad](#buenas-prácticas-y-seguridad-en-producción)

---

## 1. Conceptos Básicos y Arquitectura

### ¿Qué es un Faucet en Blockchain?
Un **Faucet** (grifo o dispensador) es un servicio que distribuye pequeñas cantidades de tokens o criptomonedas nativas (en este caso **Lamports** en Solana) para permitir a desarrolladores y usuarios probar aplicaciones sin gastar dinero real.

* **1 SOL** = `1,000,000,000` Lamports (10⁹ Lamports).
* **0.0001 SOL** = `100,000` Lamports.

### Minicrates Modulares de Solana (Solana v2)
Anteriormente, el SDK de Solana requería la gigantesca dependencia `solana-sdk`. En versiones recientes de Solana SDK (v2.x), la librería ha sido desglosada en **minicrates ligeros e independientes**:
* `solana-pubkey`: Manejo de claves públicas.
* `solana-keypair`: Generación y carga de pares de claves (Keypair) criptográficas.
* `solana-signer`: Traits de firma criptográfica.
* `solana-rpc-client`: Cliente RPC asíncrono no bloqueante para interactuar con nodos de Solana.
* `solana-transaction`: Estructuración y firma de transacciones.
* `solana-system-interface`: Instrucciones nativas del **System Program** de Solana (por ejemplo `transfer`).

---

## 2. Estructura del Proyecto

```text
solana-faucet-api-axum/
├── Cargo.toml          # Configuración del paquete y dependencias en Rust
├── .env.example        # Plantilla de variables de entorno
├── .env                # Credenciales y variables locales (NO subir a git)
├── .gitignore          # Archivos excluidos del control de versiones
└── src/
    └── main.rs         # Código fuente principal del servidor API REST
```

---

## 3. Requisitos Previos

Antes de comenzar, asegúrate de tener instalado:

1. **Rust y Cargo**:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. **Una Wallet de Solana en Devnet con fondos**:
   Puedes generar un keypair de pruebas con la Solana CLI:
   ```bash
   solana-keygen new --outfile ~/faucet-keypair.json
   solana airdrop 2 $(solana-keygen pubkey ~/faucet-keypair.json) --url devnet
   ```

---

## 4. Paso 1: Configuración de Dependencias (`Cargo.toml`)

Crea el archivo `Cargo.toml` con las siguientes dependencias:

```toml
[package]
name = "solana-faucet-api-axum"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8.9"
tokio = { version = "1.53.1", features = ["rt-multi-thread", "macros"] }
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0"

# Minicrates oficiales de Solana
solana-pubkey = "2.4.0"
solana-keypair = "2.2.3"
solana-signer = "2.2.1"
solana-rpc-client = "2.2.0"
solana-transaction = "2.2.3"
solana-system-interface = { version = "1.0.0", features = ["bincode"] }

dotenvy = "0.15"
```

---

## 5. Paso 2: Variables de Entorno (`.env`)

Crea el archivo `.env` en la raíz del proyecto para almacenar de forma segura la clave privada del Faucet y configuraciones del servidor:

```env
FAUCET_SECRET_KEY="tu_clave_privada_base58_aqui"
RPC_URL="https://api.devnet.solana.com"
PORT="3000"
AIRDROP_AMOUNT_LAMPORTS="100000"
```

> ⚠️ **Importante**: La clave privada en `FAUCET_SECRET_KEY` debe estar en formato **Base58** (el string legible de 88 caracteres aproximadamente). Nunca subas tu archivo `.env` a repositorios públicos.

---

## 6. Paso 3: Desarrollo del Servidor API REST (`src/main.rs`)

A continuación se detalla el código completo comentado paso a paso:

```rust
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
#[derive(Deserialize)]
pub struct AirdropRequest {
    pub pubkey: String,
}

/// Estructura de respuesta HTTP en formato JSON.
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
    dotenvy::dotenv().ok();

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let bind_addr = format!("0.0.0.0:{}", port);

    // Definición de rutas con Axum
    let app = Router::new()
        .route("/airdrop", post(airdrop))
        .route("/", post(airdrop));

    println!("🚀 Faucet API Server iniciado.");
    println!("📡 Escuchando en http://{}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("❌ No se pudo enlazar al puerto");

    axum::serve(listener, app)
        .await
        .expect("❌ Error al ejecutar el servidor HTTP");
}

pub async fn airdrop(
    Json(payload): Json<AirdropRequest>,
) -> (StatusCode, Json<AirdropResponse>) {
    // 1. Obtener clave privada del Faucet
    let secret_key_str = match env::var("FAUCET_SECRET_KEY") {
        Ok(key) => key,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AirdropResponse {
                    success: false,
                    message: "FAUCET_SECRET_KEY no configurada".to_string(),
                    tx_signature: None,
                    explorer_url: None,
                }),
            );
        }
    };

    // 2. Cargar Keypair del Faucet desde formato Base58
    let faucet_keypair = Keypair::from_base58_string(&secret_key_str);
    let faucet_pubkey = faucet_keypair.pubkey();

    // 3. Validar Pubkey del destinatario
    let recipient_pubkey = match Pubkey::from_str(&payload.pubkey) {
        Ok(pk) => pk,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AirdropResponse {
                    success: false,
                    message: "Dirección de Solana inválida".to_string(),
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
        .unwrap_or(100_000);

    // 4. Crear cliente RPC e instrucción de transferencia
    let rpc_client = RpcClient::new(rpc_url);
    let transfer_instruction = transfer(
        &faucet_pubkey,
        &recipient_pubkey,
        lamports_to_send,
    );

    // 5. Obtener blockhash reciente de la red
    let recent_blockhash = match rpc_client.get_latest_blockhash().await {
        Ok(hash) => hash,
        Err(err) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(AirdropResponse {
                    success: false,
                    message: format!("Error al consultar nodo RPC: {}", err),
                    tx_signature: None,
                    explorer_url: None,
                }),
            );
        }
    };

    // 6. Construir y firmar la transacción
    let transaction = Transaction::new_signed_with_payer(
        &[transfer_instruction],
        Some(&faucet_pubkey),
        &[&faucet_keypair],
        recent_blockhash,
    );

    // 7. Transmitir transacción y confirmar
    match rpc_client.send_and_confirm_transaction(&transaction).await {
        Ok(signature) => {
            let sig_str = signature.to_string();
            let explorer_link = format!(
                "https://explorer.solana.com/tx/{}?cluster=devnet",
                sig_str
            );

            (
                StatusCode::OK,
                Json(AirdropResponse {
                    success: true,
                    message: format!("Airdrop de {} lamports enviado.", lamports_to_send),
                    tx_signature: Some(sig_str),
                    explorer_url: Some(explorer_link),
                }),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AirdropResponse {
                success: false,
                message: format!("Error al procesar la transacción: {:?}", err),
                tx_signature: None,
                explorer_url: None,
            }),
        ),
    }
}
```

---

## 7. Paso 4: Compilación y Ejecución

Para iniciar el servidor localmente:

```bash
cargo run
```

Deberías ver la siguiente salida en consola:
```text
🚀 Faucet API Server iniciado exitosamente.
📡 Escuchando en http://0.0.0.0:3000
```

---

## 8. Paso 5: Pruebas de la API (curl, Postman, JS)

### Prueba con `curl`

Abre una terminal y ejecuta una solicitud HTTP POST pasando la dirección pública a la que deseas enviar el airdrop:

```bash
curl -X POST http://localhost:3000/airdrop \
  -H "Content-Type: application/json" \
  -d '{"pubkey": "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU"}'
```

#### Respuesta exitosa esperada:
```json
{
  "success": true,
  "message": "Airdrop de 100000 lamports enviado exitosamente.",
  "tx_signature": "2E9...x8Z",
  "explorer_url": "https://explorer.solana.com/tx/2E9...x8Z?cluster=devnet"
}
```

---

### Prueba con JavaScript / TypeScript (Fetch API)

```javascript
async function requestAirdrop(walletAddress) {
  const response = await fetch("http://localhost:3000/airdrop", {
    method: "POST",
    headers: {
      "Content-Type": "application/json"
    },
    body: JSON.stringify({ pubkey: walletAddress })
  });

  const data = await response.json();
  console.log("Respuesta del Faucet:", data);
}

// Ejemplo de uso:
requestAirdrop("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU");
```

---

## 🛡️ Buenas Prácticas y Seguridad en Producción

1. **Rate Limiting (Límite de solicitudes)**:
   En un entorno de producción, implementa un middleware de limitación de frecuencia (por IP o por wallet) usando crates como `tower-governor` para prevenir abuso o agotamiento de fondos del Faucet.
2. **Protección de la Clave Privada**:
   Almacena `FAUCET_SECRET_KEY` en un gestor de secretos seguro (como AWS Secrets Manager, Vault o variables de entorno en Kubernetes) y nunca en el código fuente.
3. **Manejo de CORS**:
   Si consumes esta API desde un navegador frontend, agrega middleware CORS con el crate `tower-http`.
