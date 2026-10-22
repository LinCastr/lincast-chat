# Protótipo: tela + áudio de aplicativo no Wayland (v2)

Não é o app. Responde uma pergunta antes de a reescrita começar: as peças em
Rust funcionam juntas de verdade? As peças são três:

- o vídeo da tela, pelo portal do Wayland (capturador do próprio libwebrtc);
- o áudio de aplicativos escolhidos, direto do PipeWire, sem sink virtual e
  sem o som da chamada voltar;
- o WebRTC nativo, falando o protocolo do servidor atual.

Quem assiste usa o app de sempre, no navegador, na mesma sala.

## Compilar

Você precisa de três coisas:

- **Rust**;
- **clang 21 ou mais novo**: o libwebrtc pré-compilado exige, e o build para
  com erro claro se não achar;
- **cabeçalhos do PipeWire**: no Arch e no CachyOS eles já vêm no pacote
  `pipewire`.

```bash
cd v2
cargo build --release
```

A primeira compilação baixa o libwebrtc (~100MB) e leva alguns minutos. Os
binários saem em `v2/target/release/`, e estes dois precisam ficar juntos na mesma pasta:

- `target/release/concord-proto`: o programa principal;
- `target/release/concord-audio`: a captura de áudio. O motivo de ser um
  processo à parte está em "Por que dois binários", mais abaixo.

## Testar (o que só dá pra fazer numa sessão gráfica de verdade)

1. **Deixe tocando** o que vai junto na transmissão (Spotify, um jogo, um vídeo
   no mpv) e veja como ele aparece:

   ```bash
   ./target/release/concord-proto listar
   ```

2. **Transmita** pra uma sala de teste, usando parte do nome que apareceu no
   `listar`:

   ```bash
   ./target/release/concord-proto transmitir --sala teste-v2 --app spotify
   ```

   O diálogo do sistema (portal) pede a tela ou janela. No Hyprland é o
   `xdg-desktop-portal-hyprland`.

3. **Assista de outra máquina**, ou do celular:
   1. Abra o site.
   2. Entre na sala `teste-v2`.
   3. Clique em **Entrar na chamada**.

   Tem que aparecer a tela, com o som só do app escolhido.

4. **Ctrl+C** encerra e avisa a sala que a tela parou.

Opções úteis:

| Opção | O que faz |
|---|---|
| `--app NOME` | escolhe o app pelo nome; pode repetir pra mandar mais de um |
| `--todos` | manda o som de todos os apps menos ele mesmo; se a chamada estiver tocando num navegador **na mesma máquina**, ela entra junto |
| `--sem-tela` | só áudio |
| `--fps 60` | aumenta a taxa de quadros |
| `--cursor` | inclui o cursor na imagem |
| `--x11` | sessão X11: captura a tela inteira sem portal |
| `--servidor URL` | aponta pra outro servidor; o padrão é a produção |

Se der problema, mande a saída inteira do terminal. Cada passo é registrado:
- conexão: `[ws]`, `[ice]` e as linhas por pessoa;
- tela: `[tela]` (quadros por segundo);
- áudio: `[audio]` (o que foi capturado e o que foi ignorado, e o pico de som a
  cada 5s).

## O que já foi conferido sem você (2026-10-12, sessão na nuvem)

- **Áudio por aplicativo**, com PipeWire + WirePlumber de verdade no container
  e dois "apps" falsos tocando tons diferentes:
  - `--app` captura só o escolhido (440 Hz entra, 1000 Hz fica de fora);
  - `--todos` pega os dois;
  - quando o app fecha, a captura **não** se religa em outra fonte (o
    microfone vazaria);
  - um app que começa a tocar depois também entra.
- **Interoperabilidade com a v1** (`teste-interop.js`): servidor atual local, a
  página atual num Chromium headless entrando na chamada. Passa nas duas ordens
  de chegada:
  - conexão `connected`;
  - stream reconhecida como tela;
  - som chegando com o nível exato e na frequência certa;
  - vídeo 1280x720 decodificado, com imagem e cores conferidas num quadro
    salvo.
- **O que NÃO foi conferido**: o portal do Wayland (não existe sem sessão
  gráfica) e qualquer máquina real. O vídeo foi testado pelo capturador X11
  (`--x11`, num Xvfb). O caminho do portal é o mesmo código do libwebrtc que o
  Chromium usa, mas só o seu teste prova.

Pra rodar o teste de interoperabilidade de novo, você precisa de:
- um servidor v1 local (receita no `AGENTS.md`, seção "Testes");
- um app tocando som;
- opcionalmente, um X11 pra testar o vídeo.

```bash
DISPLAY=:0 node prototipo-tela/teste-interop.js http://localhost:8099 nome-do-app
```

## Por que dois binários

O libwebrtc pré-compilado embute **stubs** com os mesmos nomes das funções do
PipeWire (`pw_init`, `pw_stream_new`...). Ele só preenche esses stubs via
`dlopen` quando a captura de tela começa. No link estático, o `pipewire-rs`
acabava chamando esses stubs ainda vazios: segfault no `pw_init` (visto com
gdb).

O `concord-audio` não linka o libwebrtc, então nele `pw_*` é a biblioteca de
verdade. Ele manda o som pro principal por um pipe, em blocos de 10ms.

Bônus: se a captura de áudio cair, a chamada continua.
