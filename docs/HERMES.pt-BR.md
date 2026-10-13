# Hermes Agent no Coucou (Windows e Linux)

*English: [HERMES.md](HERMES.md)*

"Como está minha agenda hoje?", "Coloca minha playlist de viagem no Spotify", "Chego em casa em dez minutos: liga o som
e as luzes do quarto". Com o **Hermes Agent** conectado, o chat do Coucou para Windows e Linux (a ilha, e o iPhone ou
Android pareado com o computador) passa a ser respondido pelo [Hermes Agent](https://github.com/NousResearch/hermes-agent),
o agente de código aberto da Nous Research (licença MIT), com a memória, as habilidades e as ferramentas dele: Google
Agenda e Gmail, Spotify, Home Assistant, tarefas agendadas, a web, o terminal e as centenas de integrações que ele pode
instalar.

O Coucou é o rosto e as mãos em volta dele:

- **O Mochi mostra o que o Hermes faz**, passo a passo, no chat do computador e do celular.
- **O Hermes te pergunta pelo Coucou.** Uma ação que o Hermes quer que uma pessoa aprove aparece como os pedidos do
  próprio Mochi: na ilha, ou no celular pareado com Face ID ou digital. Permitir ou Negar volta para o Hermes.
- **Sua voz**: o microfone do celular e as respostas lidas em voz alta (veja [IPHONE_PC.pt-BR.md](IPHONE_PC.pt-BR.md))
  funcionam igual com o Hermes.
- **A Casa do Mochi para o Hermes**: o Hermes pode usar os aparelhos da Casa do Mochi (TVs Samsung e Philips,
  aparelhos do Home Assistant, suas rotinas) por MCP, com as mesmas regras do Mochi ([SMART_HOME.pt-BR.md](SMART_HOME.pt-BR.md)).

O Hermes é **conectado, não copiado** para dentro do Coucou: ele é um agente grande, em Python, que se atualiza sozinho,
e o Coucou conversa com ele pelo servidor de API oficial dele. Tudo o que o Hermes aprende ou instala fica disponível na
hora, e o Coucou continua leve.

## O que você precisa

- O Coucou para Windows ou Linux, a versão depois da 0.4.0 (ainda não lançada: pegue o instalador nos artefatos do
  workflow **Windows installer**, ou compile a partir de [`windows/`](../windows/README.md)).
- O Hermes Agent **no mesmo computador** (Windows 10/11 nativo, Linux, macOS ou WSL2), com um provedor de IA
  configurado (`hermes setup`).

## 1. Instale o Hermes

No Windows, no PowerShell (não precisa de administrador):

```powershell
iex (irm https://hermes-agent.nousresearch.com/install.ps1)
```

No Linux, macOS ou WSL2: `curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash`. Abra um terminal novo,
rode `hermes setup` e escolha a IA que vai responder (Nous Portal, Anthropic, OpenAI, OpenRouter, um modelo local…).

## 2. Ligue o servidor de API dele

O Hermes guarda os arquivos dele em `%LOCALAPPDATA%\hermes\` no Windows e em `~/.hermes/` nos outros sistemas.
Acrescente no arquivo `.env` dele:

```bash
API_SERVER_ENABLED=true
API_SERVER_KEY=um-segredo-longo-que-voce-inventar
```

Depois inicie o gateway: `hermes gateway` (ou `hermes gateway install` para ele iniciar sozinho quando você entrar no
Windows). Ele fica em `http://127.0.0.1:8642`, só neste computador.

## 3. Conecte no Coucou

1. **Configurações → Hermes Agent**: deixe o endereço vazio (é `http://127.0.0.1:8642`), digite o mesmo segredo em
   **Chave de API**, **Conectar**. O Coucou confere se é mesmo o Hermes Agent e se ele sabe rodar, transmitir, parar e
   esperar uma aprovação, e mostra o nome com que ele responde.
2. **Usar no chat**, ou escolha **Hermes** em cima da caixa do chat.
3. Para o celular: **Configurações → Celular**, ligue **Permitir que o celular converse com meus provedores de IA**,
   depois **Escolher modelos** ao lado de **Hermes** e marque o nome dele.

## 4. Converse

- "Como está minha agenda hoje?" — o Hermes lê sua agenda (depois de configurada no Hermes, abaixo).
- "Coloca Legião Urbana no Spotify." — o Hermes toca no aparelho do Spotify que estiver ligado.
- "Me lembra amanhã às 9h de pagar a conta de luz." — uma tarefa agendada no Hermes.
- Do iPhone, voltando para casa: "Chego em 10 minutos: liga o som, põe minha playlist e acende a luz do quarto em
  vermelho." — com o som e as luzes no Home Assistant.

Cada ferramenta que o Hermes usa aparece no chat ("Hermes spotify_playback · play…"), e **Parar** para o Hermes.

## 5. Dê os poderes ao Hermes (no Hermes)

Isto se configura no Hermes, uma vez só; o Coucou nunca vê suas senhas do Google ou do Spotify.

- **O nome e a personalidade**: edite o `SOUL.md` na pasta do Hermes ("Você é o Zheus, …").
- **Google Agenda e Gmail**: peça ao Hermes "configure o Google Workspace": a habilidade `google-workspace` dele te
  guia no login do Google (um cliente OAuth do tipo "App para computador" no seu próprio Google Cloud).
- **Spotify**: `hermes plugins install spotify`, `hermes tools enable spotify`, `hermes spotify login`. Para tocar
  música precisa do Spotify Premium e de um app do Spotify aberto em algum aparelho (o computador, o celular, uma caixa
  com Spotify Connect).
- **Home Assistant** (luzes, ar-condicionado, caixas de som, tomadas): `hermes plugins install homeassistant`, depois
  `HASS_URL` e `HASS_TOKEN` no `.env` do Hermes.
- **Palavra de ativação no computador**: o app de desktop do próprio Hermes escuta uma (o Porcupine já vem com
  "Jarvis"; o motor `sherpa` aceita uma frase que você digita). Veja o guia "Wake word" do Hermes.
- **Um resumo de manhã no celular**: uma tarefa agendada do Hermes (`hermes cron add …`) pode entregar no ntfy, o
  mesmo app de notificações que o Coucou usa para o iPhone.

## 6. Casa do Mochi para o Hermes (MCP)

**Configurações → Hermes Agent → Casa do Mochi para o Hermes** (desligado por padrão) deixa o Hermes, e qualquer outro
agente deste computador que fale MCP, usar a Casa do Mochi. Acrescente o que a tela mostra no `config.yaml` do Hermes e
reinicie o `hermes gateway`:

```yaml
mcp_servers:
  coucou:
    command: 'C:\Users\voce\AppData\Local\Coucou\bin\coucou-hook.exe'
    args: ["--mcp"]
```

O Hermes passa a ter `home_devices`, `home_control` e `home_routine`. O `coucou-hook --mcp` não executa nada sozinho:
ele entrega cada chamada ao Coucou, que aplica as regras do Mochi — só os aparelhos marcados para o Mochi, só as ações
que cada um informou, um clique (Face ID no celular) para tomadas, interruptores e aparelhos marcados para perguntar, e
cada ação no registro da casa como "Hermes Agent". Os braços do Mochi (arquivos, comandos) nunca ficam acessíveis por
aqui. Com o Coucou fechado, o Hermes é avisado.

## Segurança

- **A chave** fica no cofre de senhas do sistema (Gerenciador de Credenciais do Windows, Secret Service no Linux) junto
  com o endereço para o qual foi digitada, e só vai para lá: para este computador, ou por https. Nunca em arquivo,
  nunca nos logs.
- **O que vai para o Hermes**: sua mensagem, o texto de um arquivo que você soltou no chat, e uma nota curta dizendo de
  onde vem a mensagem. Nenhuma outra chave, nenhuma configuração.
- **Aprovações**: o Coucou mostra a pergunta do Hermes com o motivo ("apagar recursivamente"); sem resposta em uns
  quatro minutos e meio, ele responde não (o próprio Hermes espera cinco minutos por padrão). **Parar** pede ao Hermes
  para parar.
- **O que o Hermes pode fazer sozinho** é a política do próprio Hermes (as configurações `approvals` dele): o Coucou
  mostra cada ferramenta que ele usa e te pergunta tudo o que o Hermes pergunta. Leia o guia de segurança do Hermes
  antes de dar acesso a coisas importantes.
- **Uma coisa de cada vez**: uma conversa com o Hermes e uma dos braços do Mochi usam o mesmo lugar; a segunda espera a
  primeira.

## Limites, com sinceridade

- **Testado com um Hermes simulado**: as tarefas do servidor de API, os eventos, as aprovações e o parar, e um cliente
  MCP, ainda não com um Hermes de verdade. A API do Hermes é documentada e versionada (`/v1/capabilities`); o Coucou
  recusa um servidor que não é o Hermes ou que é antigo demais.
- **O Hermes no mesmo computador** (ou acessível por https). A chave nunca vai sem criptografia para outra máquina.
- **A qualidade das respostas** é a do modelo que você escolheu no Hermes.
- O plugin do Hermes em **Configurações → Agentes** (as sessões do Hermes na pílula dele) continua funcionando e pode
  ficar ligado junto.

## Para desenvolvedores

Veja a seção "For developers" de [HERMES.md](HERMES.md).
