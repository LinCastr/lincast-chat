# Lincast Chat v2: cliente desktop

App nativo em Rust (interface em [iced](https://iced.rs), WebRTC pelo
`libwebrtc` da LiveKit). Ele conversa com o **servidor atual** (`server.js`),
então quem usa o app novo e quem usa o site se encontram na mesma sala.

## O que já funciona

- **Conta:** entrar e criar conta (usuário e senha). A sessão fica salva em
  `~/.config/lincast-chat/sessao`, com modo 600.
- **Salas:** lista de salas ativas, atualizada a cada 4s, e entrar por nome com
  senha opcional. Senha errada volta pra lista com o motivo.
- **Dentro da sala:**
  - quem está na chamada e quem está só no chat, com ícones de mudo e surdo;
  - chat com histórico, imagens recebidas e mensagens agrupadas por autor.
- **Chamada:**
  - entrar e sair da chamada, mudo e ensurdecer;
  - microfone e alto-falante do sistema, com o cancelamento de eco do WebRTC.
- **Tela:**
  - compartilhar a tela escolhendo quais apps têm o som enviado junto
    (`concord-audio`);
  - ver a tela e a câmera dos outros.

## Compilar

Precisa de três coisas:
- **Rust**;
- **clang 21 ou mais novo**: o libwebrtc pré-compilado exige;
- **cabeçalhos do PipeWire**: no Arch e no CachyOS vêm no pacote `pipewire`.

```bash
cd v2
cargo build --release
```

Compile **o workspace inteiro**, não só este pacote: o app chama o
`concord-audio` (outro pacote), que precisa sair atualizado na mesma pasta
`target/release/`. Com o binário velho, a lista de apps do compartilhamento de
tela falha. O app avisa quando isso acontece.

## Rodar

```bash
./target/release/lincast-chat
```

Por padrão ele usa o servidor de produção. Pra testar contra um servidor local,
defina a variável `LINCAST_SERVIDOR`:

```bash
LINCAST_SERVIDOR=http://localhost:8099 ./target/release/lincast-chat
```

**Ícone na barra de tarefas (Wayland):** lá o ícone vem de um arquivo
`.desktop` com o mesmo id do app (`lincast-chat`), não da janela. Por exemplo, em
`~/.local/share/applications/lincast-chat.desktop`:

```ini
[Desktop Entry]
Name=Lincast Chat
Exec=/caminho/para/v2/target/release/lincast-chat
Icon=/caminho/para/logo/concord-icone.png
Type=Application
Categories=Network;Chat;
```
