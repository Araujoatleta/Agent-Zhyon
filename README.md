<div align="center">

<img src="NotchBuddy/Assets.xcassets/AppIcon.appiconset/icon_256x256.png" width="96" alt="Ícone do Agent-Zhyon">

# Agent-Zhyon

### Seu assistente de inteligência artificial, sempre por perto.

**Acompanhe seus agentes de IA, converse, aprove solicitações e organize tarefas sem interromper o que está fazendo.** No Windows e Linux, o assistente fica na parte superior da tela; no macOS, pode ocupar a região da câmera. Há também recursos de integração com dispositivos móveis.

[![Windows 10/11](https://img.shields.io/badge/Windows-10%2F11-0078D4?logo=windows&logoColor=white)](windows/README.md)
![macOS 15+](https://img.shields.io/badge/macOS-15%2B-black?logo=apple)
![iOS 18+](https://img.shields.io/badge/iOS-18%2B-black?logo=apple)
![Android 11+](https://img.shields.io/badge/Android-11%2B-3DDC84?logo=android&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-AppImage%20%7C%20deb%20%7C%20rpm-FCC624?logo=linux&logoColor=black)
![Tauri 2](https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=black)
![Código MIT](https://img.shields.io/badge/c%C3%B3digo-MIT-green)

### [⬇️ BAIXAR O AGENT-ZHYON PARA WINDOWS](https://github.com/Araujoatleta/Agent-Zhyon/releases)

[Baixar o instalador Windows selecionado (artefato temporário)](https://github.com/Araujoatleta/Agent-Zhyon/actions/runs/38667992483/artifacts/11942537867)

<img src="docs/media/demo.gif" width="760" alt="Demonstração animada do assistente">

</div>

---

## Conheça o Agent-Zhyon

O **Agent-Zhyon** é um projeto de personalização e desenvolvimento de um assistente de IA com interface compacta. O personagem animado **Mochi** acompanha suas atividades e mostra o que seus agentes estão fazendo. É possível abrir o chat, consultar atividades, receber alertas e, nas plataformas compatíveis, autorizar ações no computador.

O projeto reúne interfaces para Windows, Linux, macOS e dispositivos móveis. Algumas funções exigem configuração adicional, serviços de terceiros ou versões específicas do aplicativo.

## Principais funcionalidades

- 🤖 **Acompanhamento de agentes:** Claude Code, Cursor, Codex, Gemini CLI, Antigravity, Copilot CLI, Muse Code, OpenCode, Amp, Hermes e outros agentes compatíveis. Veja sessões, arquivos acessados, comandos e resultados.
- ✅ **Aprovações na interface:** responda a solicitações de permissão e perguntas dos agentes, quando houver integração disponível.
- 🦾 **Controle do computador:** no Windows e Linux, o recurso *Mãos do Mochi* permite interagir com arquivos, comandos e páginas da web mediante permissões. [Veja como funciona](windows/README.md#mochis-hands).
- 🪽 **Hermes Agent:** use o Hermes como agente de IA, com suas ferramentas e recursos de memória, conforme a configuração. [Guia do Hermes em português](docs/HERMES.pt-BR.md).
- 🏠 **Casa inteligente:** integração opcional com dispositivos compatíveis e serviços de automação residencial. [Documentação em português](docs/SMART_HOME.pt-BR.md).
- 💬 **Chat com IA:** converse com os provedores e modelos compatíveis com a plataforma; alguns exigem chave de API ou servidor local.
- 📊 **Acompanhamento de uso:** consulte informações de sessões e limites dos agentes quando a integração oferecer esse recurso.
- 📎 **Arquivos no chat:** arraste arquivos para a interface em plataformas compatíveis.
- 🖥️ **Personagem na área de trabalho:** movimente Mochi pela tela e personalize sua aparência.
- 🔌 **Integrações:** serviços como GitHub, n8n, Vercel, Notion, Stripe, Resend e Cal.com, conforme disponibilidade.
- 🎵 **Música:** recursos de controle de Apple Music e Spotify em versões compatíveis.
- ⌨️ **Atalhos:** configure ações rápidas para abrir o chat e acessar recursos.
- 🎭 **Animações e sons:** Mochi reage a interações, alertas e atividades.
- 🗣️ **Voz e ditado:** disponíveis em versões compatíveis, sujeitos às permissões do sistema.
- 🔒 **Privacidade:** as credenciais são armazenadas pelos mecanismos seguros do sistema operacional; as conexões com provedores externos dependem dos serviços configurados.

## Demonstrações e imagens

As mídias abaixo são carregadas diretamente dos arquivos deste repositório.

<table>
<tr>
<td><img src="docs/media/claude-code.png" alt="Acompanhamento de uma sessão de IA"></td>
<td><img src="docs/media/stripe.png" alt="Painel de integração"></td>
</tr>
<tr>
<td><img src="docs/media/chat.png" alt="Conversa com o assistente"></td>
<td><img src="docs/media/dizzy.png" alt="Animação do personagem"></td>
</tr>
</table>

## Agent-Zhyon no iPhone

No iPhone, os recursos compatíveis permitem acompanhar sessões e receber solicitações sem ficar diante do computador.

<table>
<tr><td colspan="3" align="center"><img src="docs/media/iphone-live-activity.jpg" width="520" alt="Atividade ao vivo na tela bloqueada"></td></tr>
<tr>
<td width="33%"><img src="docs/media/iphone-approval.jpg" alt="Autorização no iPhone"></td>
<td width="33%"><img src="docs/media/iphone-session.jpg" alt="Sessão de agente no iPhone"></td>
<td width="33%"><img src="docs/media/iphone-widgets.jpg" alt="Widgets do iPhone"></td>
</tr>
</table>

- **Atividades ao Vivo e Ilha Dinâmica:** acompanhe o andamento dos agentes na tela bloqueada, quando disponível.
- **Notificações interativas:** responda a solicitações e perguntas compatíveis.
- **Face ID:** confirmação biométrica para autorizações suportadas.
- **Sessões e conversas:** acompanhe etapas e envie instruções.
- **Widgets e Atalhos:** consulte informações sem abrir o aplicativo.
- **Sincronização:** a integração com macOS pode usar o iCloud; o pareamento com Windows e Linux pode usar conexão direta à rede.

Para configurar, consulte [iPhone](docs/IPHONE.md) ou [pareamento entre iPhone e PC em português](docs/IPHONE_PC.pt-BR.md). A disponibilidade do aplicativo móvel depende do método de instalação e da versão.

## Agent-Zhyon no Android

O aplicativo Android permite acompanhar agentes e responder a solicitações, quando conectado a um computador compatível.

- **Aprovações pelo celular:** confirme solicitações com a autenticação disponível no dispositivo.
- **Sessões em tempo real:** visualize o agente ativo e outras sessões.
- **Chat e respostas:** converse usando as configurações do computador.
- **Interface flutuante:** acompanhe o assistente por notificações e elementos de tela compatíveis.
- **Pareamento local:** conexão pela rede Wi-Fi mediante código de pareamento.

**Requisitos indicados:** Android 11 ou posterior e aplicativo compatível no Windows ou Linux. Consulte [o guia do Android](android/README.md) para compilação e configuração.

## Download e instalação

### Windows 10 e 11

**[📥 Acessar os instaladores do Agent-Zhyon](https://github.com/Araujoatleta/Agent-Zhyon/releases)**

**[📦 Baixar o artefato Windows escolhido](https://github.com/Araujoatleta/Agent-Zhyon/actions/runs/38667992483/artifacts/11942537867)** — o GitHub Actions disponibiliza esse arquivo por tempo limitado e pode exigir login. O artefato foi compilado no commit `461fa27`, diferente do commit `35429f5` que serviu de base para a restauração da branch `main`.

1. Abra a página de downloads.
2. Escolha um instalador `.exe` ou `.msi` disponibilizado para a versão desejada.
3. Verifique a origem e as informações de segurança do arquivo.
4. Execute o instalador e siga as instruções.

**Atenção:** instaladores sem assinatura digital podem apresentar alertas do Windows. Verifique o arquivo antes de decidir se deseja executá-lo; não ignore avisos de segurança automaticamente.

No Windows, a interface fica na parte superior da tela. Consulte [o guia do Windows](windows/README.md) para diferenças entre plataformas e recursos de controle do computador.

### Linux

Consulte as [versões publicadas neste repositório](https://github.com/Araujoatleta/Agent-Zhyon/releases) e o [diretório Linux](linux/). Conforme o pacote disponibilizado, a instalação pode usar AppImage, `.deb` ou `.rpm`.

### macOS

Consulte as [versões publicadas neste repositório](https://github.com/Araujoatleta/Agent-Zhyon/releases). A instalação depende da disponibilização de uma compilação compatível com o macOS.

### iPhone e Android

Consulte a [documentação do iPhone](docs/IPHONE.md), o [guia de pareamento iPhone–PC](docs/IPHONE_PC.pt-BR.md) e a [documentação Android](android/README.md). Não há promessa de publicação própria em lojas de aplicativos até que uma versão correspondente esteja disponível.

## Compilar a partir do código-fonte

### Windows

**Pré-requisitos:** Rust, Node.js 20 ou superior e ferramentas de compilação MSVC.

```powershell
git clone https://github.com/Araujoatleta/Agent-Zhyon.git
cd Agent-Zhyon/windows
npm install
npm run pack
```

### macOS

**Pré-requisitos:** macOS 15 ou superior, Xcode 16 ou superior e XcodeGen.

```bash
brew install xcodegen
git clone https://github.com/Araujoatleta/Agent-Zhyon.git
cd Agent-Zhyon/NotchBuddy
xcodegen
open NotchBuddy.xcodeproj
```

### Linux

**Pré-requisitos:** Rust, Node.js 20 ou superior e bibliotecas de desenvolvimento do WebKitGTK e ambiente gráfico.

```bash
git clone https://github.com/Araujoatleta/Agent-Zhyon.git
cd Agent-Zhyon/windows
npm install
npm run pack
```

### Android

**Pré-requisitos:** JDK 17 e Android SDK (plataforma 35).

```bash
git clone https://github.com/Araujoatleta/Agent-Zhyon.git
cd Agent-Zhyon/android
./gradlew assembleDebug
```

## Configuração inicial

Abra o ícone do aplicativo na barra de menus ou bandeja do sistema e acesse **Configurações**. Os nomes dos menus podem variar conforme a versão e o idioma instalado.

| Recurso | Finalidade | Configuração |
|---|---|---|
| Claude Code | Sessões e permissões | Instalar hooks nas configurações do agente |
| Codex | Acompanhamento de sessões | Configurar a integração com Codex |
| Hermes Agent | Chat e ferramentas | Consultar o [guia do Hermes](docs/HERMES.pt-BR.md) |
| Chaves de API | Acesso aos provedores | Informar as chaves nos campos correspondentes |
| Modelos locais | Uso de modelos compatíveis | Configurar Ollama ou LM Studio, quando suportado |
| Celular | Pareamento com o computador | Ativar o vínculo e seguir o guia da plataforma |
| Serviços externos | Integrações opcionais | Conectar apenas os serviços que deseja utilizar |

## Agentes compatíveis

| Agente | Integração |
|---|---|
| Claude Code | Hooks de eventos e permissões |
| Cursor | Integração com eventos compatíveis |
| Codex | Integração e acompanhamento de sessões |
| Gemini CLI | Hooks, conforme a plataforma |
| Antigravity | Hooks, conforme a plataforma |
| Copilot CLI | Eventos de agente |
| Muse Code | Eventos de agente |
| OpenCode | Plugin compatível |
| Amp | Plugin compatível |
| Hermes | Plugin e recursos específicos |
| Outros | Integração mediante protocolo de eventos compatível |

Para ampliar o suporte, consulte [a documentação dos agentes](docs/AGENTS.md).

## Experimente estas interações

| Ação | Resultado esperado |
|---|---|
| Passar o cursor na parte superior da tela | O assistente aparece, conforme a configuração |
| Clicar na interface | Abre o painel |
| Interagir com Mochi | O personagem reage com animações |
| Arrastar um arquivo para a interface | O arquivo pode ser anexado, quando suportado |
| Selecionar um agente | Exibe sessões e informações disponíveis |
| Abrir o chat | Permite conversar com o provedor configurado |
| Ativar as mãos do Mochi | Disponibiliza ferramentas autorizadas para interação com o computador |

## Como funciona

**Windows:** aplicativo Tauri 2, com Rust e TypeScript, interface transparente e integração com o sistema operacional. O acompanhamento de sessões usa mecanismos de eventos e hooks.

**Linux:** utiliza a mesma base do aplicativo desktop, com integração à interface gráfica e comunicação local compatível com o sistema.

**macOS:** interface nativa em Swift e SwiftUI, com animações, integrações de agentes e recursos do sistema Apple.

**iPhone:** aplicativo nativo com integração às sessões e notificações, conforme a forma de conexão.

**Android:** aplicativo em Kotlin e Jetpack Compose, com pareamento ao computador e recursos móveis de acompanhamento.

## Organização do repositório

- [`windows/`](windows/) — aplicativo desktop para Windows e base compartilhada com Linux.
- [`linux/`](linux/) — recursos específicos de Linux.
- [`NotchBuddy/`](NotchBuddy/) — aplicativo para macOS.
- [`android/`](android/) — aplicativo Android.
- [`docs/`](docs/) — documentação e guias.
- [`relay/`](relay/) — componentes de comunicação e integração.
- [`CHANGELOG.md`](CHANGELOG.md) — histórico de alterações.

## Contribuições

Sugestões, correções e melhorias podem ser propostas por meio das [Issues](https://github.com/Araujoatleta/Agent-Zhyon/issues) e [Pull Requests](https://github.com/Araujoatleta/Agent-Zhyon/pulls) deste repositório. Consulte [as orientações para contribuir](CONTRIBUTING.md).

## Licenças e direitos autorais

O **Agent-Zhyon** é uma personalização baseada no projeto de código aberto Coucou. O código original e as contribuições de terceiros permanecem sujeitos às respectivas licenças e avisos de direitos autorais. A licença MIT permite reutilização e modificações, desde que os avisos exigidos sejam preservados.

**Atenção aos recursos visuais:** nome, personagem Mochi, ícones, sons e mídias podem ter condições próprias e não são automaticamente liberados pela licença do código. Antes de redistribuir instaladores ou publicar uma marca própria com esses recursos, consulte [LICENSE](LICENSE), [LICENSE-ASSETS.md](LICENSE-ASSETS.md) e [android/LICENSE](android/LICENSE), quando aplicável.

---

<div align="center">

**Agent-Zhyon** · Personalização e manutenção: **Araujoatleta**

[Repositório](https://github.com/Araujoatleta/Agent-Zhyon) · [Downloads](https://github.com/Araujoatleta/Agent-Zhyon/releases) · [Documentação](docs/)

</div>
