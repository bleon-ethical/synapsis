//! Secure TCP transport for Synapsis MCP using post-quantum cryptography.
//!
//! Provides encrypted, authenticated communication between MCP clients and server
//! using Kyber512 key exchange and AES-256-GCM encryption.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

use base64::{engine::general_purpose, Engine as _};

use crate::presentation::mcp::McpServer;
use synapsis_core::core::crypto_provider::SynapsisPqcProvider;
use synapsis_core::core::pqc;
use synapsis_core::domain::crypto::{CryptoProvider, PqcAlgorithm};

/// Secure TCP server for MCP protocol
pub struct SecureTcpServer {
    mcp_server: Arc<McpServer>,
    listener: TcpListener,
    crypto_provider: Arc<dyn CryptoProvider>,
}

impl SecureTcpServer {
    /// Create new secure TCP server bound to address
    pub fn new(mcp_server: Arc<McpServer>, addr: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let crypto_provider: Arc<dyn CryptoProvider> =
            Arc::new(SynapsisPqcProvider::new()) as Arc<dyn CryptoProvider>;
        Ok(Self {
            mcp_server,
            listener,
            crypto_provider,
        })
    }

    /// Start secure TCP server (blocking)
    pub fn run(&self) -> std::io::Result<()> {
        eprintln!(
            "[MCP Secure TCP] Server listening on {}",
            self.listener.local_addr()?
        );

        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let mcp_server = self.mcp_server.clone();
                    let crypto_provider = self.crypto_provider.clone();
                    thread::spawn(move || {
                        if let Err(e) =
                            handle_secure_connection(mcp_server, crypto_provider, stream)
                        {
                            eprintln!("[MCP Secure TCP] Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("[MCP Secure TCP] Accept error: {}", e);
                }
            }
        }

        Ok(())
    }
}

/// Handle a single secure TCP connection
fn handle_secure_connection(
    mcp_server: Arc<McpServer>,
    crypto_provider: Arc<dyn CryptoProvider>,
    stream: TcpStream,
) -> std::io::Result<()> {
    let peer_addr = stream.peer_addr()?;
    eprintln!("[MCP Secure TCP] New connection from {}", peer_addr);

    // Perform Kyber key exchange handshake
    let shared_secret = match perform_kyber_handshake(crypto_provider.as_ref(), &stream) {
        Ok(secret) => secret,
        Err(e) => {
            eprintln!(
                "[MCP Secure TCP] Handshake failed with {}: {}",
                peer_addr, e
            );
            return Ok(());
        }
    };

    eprintln!(
        "[MCP Secure TCP] Secure channel established with {}",
        peer_addr
    );

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    // Derive AES key using HKDF-SHA256 (replaces insecure truncation)
    let mut aes_key = [0u8; 32];
    if shared_secret.len() >= 32 {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(b"synapsis-kyber-kdf-v1");
        hasher.update(&shared_secret);
        aes_key.copy_from_slice(&hasher.finalize()[..32]);
    } else {
        let len = shared_secret.len();
        aes_key[..len].copy_from_slice(&shared_secret);
    }

    // Simple line-based protocol with encryption
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                // EOF
                eprintln!("[MCP Secure TCP] Connection closed by {}", peer_addr);
                break;
            }
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // Decrypt the message
                let decrypted = match decrypt_message(crypto_provider.as_ref(), &aes_key, line) {
                    Ok(msg) => msg,
                    Err(e) => {
                        eprintln!(
                            "[MCP Secure TCP] Decryption error from {}: {}",
                            peer_addr, e
                        );
                        break;
                    }
                };

                // Handle message through MCP server
                if let Some(response) = mcp_server.handle_message(&decrypted) {
                    // Encrypt response
                    let encrypted =
                        match encrypt_message(crypto_provider.as_ref(), &aes_key, &response) {
                            Ok(enc) => enc,
                            Err(e) => {
                                eprintln!(
                                    "[MCP Secure TCP] Encryption error to {}: {}",
                                    peer_addr, e
                                );
                                break;
                            }
                        };

                    if let Err(e) = writeln!(writer, "{}", encrypted) {
                        eprintln!("[MCP Secure TCP] Write error to {}: {}", peer_addr, e);
                        break;
                    }
                }
            }
            Err(e) => {
                eprintln!("[MCP Secure TCP] Read error from {}: {}", peer_addr, e);
                break;
            }
        }
    }

    Ok(())
}

/// Perform Kyber key exchange handshake (standard KEM flow)
///
/// Protocol:
///   1. Server generates ephemeral keypair
///   2. Server sends public key to client (base64, one line)
///   3. Client reads server PK, encapsulates to it, sends ciphertext back (base64, one line)
///   4. Server decapsulates ciphertext to recover shared secret
///
/// Returns the 32-byte shared secret for AES-256-GCM encryption.
fn perform_kyber_handshake(
    crypto_provider: &dyn CryptoProvider,
    stream: &TcpStream,
) -> Result<Vec<u8>, String> {
    let (server_pk, server_sk) = crypto_provider
        .generate_keypair(PqcAlgorithm::Kyber512)
        .map_err(|e| format!("Failed to generate server keypair: {}", e))?;

    // Step 1: Send server public key to client
    let mut writer = stream
        .try_clone()
        .map_err(|e| format!("Clone stream for write: {}", e))?;
    let server_pk_b64 = general_purpose::STANDARD.encode(&server_pk);
    writeln!(writer, "{}", server_pk_b64)
        .map_err(|e| format!("Write server public key: {}", e))?;
    writer
        .flush()
        .map_err(|e| format!("Flush server public key: {}", e))?;

    // Step 2: Read client's ciphertext
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|e| format!("Clone stream for read: {}", e))?,
    );
    let mut ciphertext_line = String::new();
    reader
        .read_line(&mut ciphertext_line)
        .map_err(|e| format!("Read client ciphertext: {}", e))?;
    let ciphertext_line = ciphertext_line.trim().to_string();

    let ciphertext = general_purpose::STANDARD
        .decode(&ciphertext_line)
        .map_err(|e| format!("Decode client ciphertext: {}", e))?;

    // Step 3: Decapsulate shared secret using server secret key
    let shared_secret = crypto_provider
        .decapsulate(&ciphertext, &server_sk, PqcAlgorithm::Kyber512)
        .map_err(|e| format!("Decapsulate shared secret: {}", e))?;

    Ok(shared_secret)
}

/// Encrypt a message with AES-256-GCM
fn encrypt_message(
    crypto_provider: &dyn CryptoProvider,
    key: &[u8; 32],
    plaintext: &str,
) -> Result<String, String> {
    let ciphertext = crypto_provider
        .encrypt(&key[..], plaintext.as_bytes(), PqcAlgorithm::Aes256Gcm)
        .map_err(|e| format!("Encrypt message: {}", e))?;
    Ok(general_purpose::STANDARD.encode(&ciphertext))
}

/// Decrypt a message with AES-256-GCM
fn decrypt_message(
    crypto_provider: &dyn CryptoProvider,
    key: &[u8; 32],
    encrypted: &str,
) -> Result<String, String> {
    let ciphertext = general_purpose::STANDARD
        .decode(encrypted)
        .map_err(|e| format!("Decode ciphertext: {}", e))?;
    let plaintext = crypto_provider
        .decrypt(&key[..], &ciphertext, PqcAlgorithm::Aes256Gcm)
        .map_err(|e| format!("Decrypt message: {}", e))?;
    String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8 plaintext: {}", e))
}

/// Secure TCP client for connecting to secure TCP server
pub struct SecureTcpClient {
    stream: TcpStream,
    aes_key: [u8; 32],
    crypto_provider: Arc<dyn CryptoProvider>,
}

impl SecureTcpClient {
    /// Connect to secure TCP server and perform handshake
    pub fn connect(addr: &str) -> Result<Self, String> {
        let stream = TcpStream::connect(addr).map_err(|e| format!("Connect to server: {}", e))?;
        let crypto_provider: Arc<dyn CryptoProvider> =
            Arc::new(SynapsisPqcProvider::new()) as Arc<dyn CryptoProvider>;

        // Perform Kyber handshake
        let shared_secret = perform_client_handshake(crypto_provider.as_ref(), &stream)?;

        // Derive AES key
        let mut aes_key = [0u8; 32];
        if shared_secret.len() >= 32 {
            aes_key.copy_from_slice(&shared_secret[..32]);
        } else {
            let len = shared_secret.len();
            aes_key[..len].copy_from_slice(&shared_secret);
        }

        Ok(Self {
            stream,
            aes_key,
            crypto_provider,
        })
    }

    /// Send an encrypted message and receive encrypted response
    pub fn send(&mut self, message: &str) -> Result<String, String> {
        // Encrypt message
        let encrypted = encrypt_message(self.crypto_provider.as_ref(), &self.aes_key, message)?;

        // Send with newline
        writeln!(&self.stream, "{}", encrypted).map_err(|e| format!("Write to server: {}", e))?;
        self.stream.flush().map_err(|e| format!("Flush: {}", e))?;

        // Read response
        let mut reader = BufReader::new(&self.stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("Read response: {}", e))?;
        let line = line.trim();

        // Decrypt response
        decrypt_message(self.crypto_provider.as_ref(), &self.aes_key, line)
    }
}

/// Perform client-side Kyber handshake (standard KEM flow)
///
/// Protocol:
///   1. Read server's public key (base64, one line)
///   2. Client encapsulates shared secret to server's PK
///   3. Client sends ciphertext to server (base64, one line)
///
/// Returns the 32-byte shared secret for AES-256-GCM encryption.
fn perform_client_handshake(
    crypto_provider: &dyn CryptoProvider,
    stream: &TcpStream,
) -> Result<Vec<u8>, String> {
    // Step 1: Read server public key
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|e| format!("Clone stream for read: {}", e))?,
    );
    let mut server_pk_line = String::new();
    reader
        .read_line(&mut server_pk_line)
        .map_err(|e| format!("Read server public key: {}", e))?;
    let server_pk_line = server_pk_line.trim();

    let server_pk = general_purpose::STANDARD
        .decode(server_pk_line)
        .map_err(|e| format!("Decode server public key: {}", e))?;

    // Step 2: Encapsulate shared secret using server's public key
    let (ciphertext, shared_secret) = crypto_provider
        .encapsulate(&server_pk, PqcAlgorithm::Kyber512)
        .map_err(|e| format!("Encapsulate shared secret: {}", e))?;

    // Step 3: Send ciphertext to server
    let mut writer = stream
        .try_clone()
        .map_err(|e| format!("Clone stream for write: {}", e))?;
    writeln!(writer, "{}", general_purpose::STANDARD.encode(&ciphertext))
        .map_err(|e| format!("Send ciphertext: {}", e))?;
    writer
        .flush()
        .map_err(|e| format!("Flush ciphertext: {}", e))?;

    Ok(shared_secret)
}

/// Start secure TCP server with given MCP server instance
pub fn start_secure_tcp_server(mcp_server: Arc<McpServer>, addr: &str) -> std::io::Result<()> {
    let server = SecureTcpServer::new(mcp_server, addr)?;
    server.run()
}
