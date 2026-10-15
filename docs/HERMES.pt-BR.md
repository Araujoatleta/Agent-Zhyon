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

## O jeito mais simples: o Coucou instala e liga tudo (Zheus)

Você não precisa abrir terminal nem editar arquivo:

1. No Coucou: **Configurações → Hermes Agent**. Em **O nome do seu assistente**, escreva o nome que vai chamar, por
   exemplo **Zheus**; escolha **O tom dele** (veja abaixo) e clique em **Salvar**.
2. Em **O cérebro dele**, escolha **ChatGPT (sua assinatura)** para usar o seu plano ChatGPT Plus ou Pro (veja abaixo),
   ou uma das IAs que o Coucou já tem (a chave da Anthropic, OpenAI, Google ou OpenRouter que você salvou nas
   Configurações, ou o Ollama ou LM Studio do computador). Deixe marcado **Deixar ele usar a casa do Mochi** se quiser
   que ele controle os aparelhos que você marcou, e marque os **pacotes de skills** que quiser (veja abaixo).
3. Leia **O que o Coucou vai fazer** e clique em **Instalar o Hermes**. O Coucou:
   - baixa o instalador oficial do Hermes do GitHub, numa versão fixa, e **confere o SHA-256** antes de rodar
     qualquer coisa (se não bater, nada roda);
   - instala o Hermes em `%LOCALAPPDATA%\hermes` (no Linux, `~/.hermes`) com tudo o que ele precisa, sem perguntar
     nada, mostrando cada etapa (1 GB e 5 a 15 minutos, mais ou menos). As ferramentas de navegador e de controle da
     tela ficam de fora (o comando `hermes pm install` adiciona depois);
   - configura o cérebro e a casa do Mochi com o próprio `hermes config set`. **A chave de IA nunca vai para um
     arquivo**: ela fica no cofre do sistema e só é entregue ao Hermes na memória, quando o Coucou o inicia. Com o
     ChatGPT não existe chave: o Coucou faz você entrar na sua conta (veja abaixo);
   - grava o nome e o tom no `SOUL.md` do Hermes (um que já exista ganha uma cópia de segurança antes) e instala os
     pacotes de skills que você marcou;
   - cria uma chave particular entre o Coucou e o Hermes e liga o servidor do Hermes **só neste computador**
     (`127.0.0.1:8642`), sempre que o Coucou estiver aberto. Se ele parar, o Coucou liga de novo (algumas vezes), e
     fechar o Coucou desliga o Hermes junto;
   - conecta o chat, o celular pareado e a casa do Mochi ao Hermes.
4. Pronto: fale com ele no chat da ilha ou no iPhone: *"Zheus, como está minha agenda hoje?"*.

**Rodar o Hermes com o Coucou** liga e desliga o servidor (o Hermes continua instalado). O Hermes também tem
comandos próprios: `hermes setup` para mais provedores, `hermes plugins install spotify` e o resto da seção 5.

Se o **Hermes já estiver instalado** neste computador, o botão vira **Deixar o Coucou rodar o Hermes**: o Coucou só
roda o seu Hermes, sem mudar as configurações dele (a não ser que você marque **Dar também este cérebro a ele**). Se o
servidor do seu Hermes já estiver rodando por conta própria, o Coucou não abre um segundo (o Hermes roda um por
computador): ele usa a chave do `.env` do Hermes, ou você conecta pelo formulário abaixo.

**Trocar o cérebro dele** dá outro cérebro ao Hermes que o Coucou roda, depois do mesmo tipo de plano: o Coucou para o
servidor dele, configura o novo cérebro e liga de novo com a nova chave (ou faz você entrar no ChatGPT).

Para tirar o Hermes depois: desligue **Rodar o Hermes com o Coucou** e rode `hermes uninstall`.

### ChatGPT: a sua assinatura, sem chave

Escolha **ChatGPT (sua assinatura)** como cérebro para usar o seu plano **ChatGPT Plus ou Pro** no lugar de uma chave de
API. É o próprio login do Hermes na OpenAI (`hermes auth add openai-codex`):

- durante a configuração, depois que o Hermes estiver instalado, o Coucou abre a página da OpenAI
  (`https://auth.openai.com/codex/device`) e mostra um código, grande, nas Configurações, com **Copiar código**,
  **Abrir a página** e **Cancelar**. Entre lá com a sua conta do ChatGPT e digite o código: ele vale por 15 minutos.
  Só a página da própria OpenAI é aberta;
- o Hermes guarda o login no `auth.json` dele, na pasta do Hermes. **O Coucou nunca lê esse arquivo** e nunca vê a sua
  senha: nada dele vai para as configurações do Coucou, para o log ou para a ligação com o celular;
- **O modelo dele**: os modelos da assinatura, do melhor para o mais simples (`gpt-6-sol`, `gpt-6-luna`, `gpt-5.6-sol`,
  `gpt-5.6-terra`, `gpt-5.6-luna`, `gpt-5.5`, `gpt-5.4-mini`, `gpt-5.4`, `gpt-5.3-codex-spark`, a lista do próprio Hermes),
  ou **Outro modelo…** para digitar um. O Hermes reinicia com ele. Quais o seu plano alcança é a OpenAI quem decide; o
  `gpt-5.6-sol` é o padrão;
- depois, **Entrar com o ChatGPT** e **Sair** ficam embaixo do cérebro dele. Cancelar encerra um login, e o Hermes não
  guarda nada dele. Os limites são os do seu plano (uma janela de 5 horas e outra semanal): `hermes usage` mostra.

### Usar o computador (tela, mouse, teclado)

**Deixar ele usar este computador** faz o Hermes ver a tela e clicar, rolar e digitar nos seus apps e sites, em segundo
plano, sem tirar o seu mouse ("abre o YouTube e coloca o trailer"): é a ferramenta `computer_use` do próprio Hermes, pelo
[cua-driver](https://github.com/trycua/cua) (Windows, macOS e Linux). Ligar roda os comandos do próprio Hermes
(`hermes tools post-setup cua_driver` e `hermes tools enable computer_use --platform api_server`: a ferramenta vem
desligada no servidor de API que o Coucou usa) e reinicia o Hermes; desligar tira a ferramenta e deixa o cua-driver
instalado. O que o Hermes faz na tela espera o seu OK, como as outras ações dele: Permitir na ilha ou no celular. Vem
desligado.

Com ele ligado, o navegador próprio do Hermes (as ferramentas `browser_*`, que rodam escondidas de você) também é
desligado no servidor de API (`hermes tools disable browser --platform api_server`; desligar a chave devolve ele), e o
Coucou ensina ao Hermes o caminho curto a cada mensagem: achar o link exato (uma busca na web), abrir no seu próprio
navegador com um comando do terminal (`cmd.exe /c start "" "<link>"` no Windows, `xdg-open` no Linux; um link de vídeo
do YouTube já começa a tocar sozinho) e usar o mouse e o teclado só para o que um link não resolve. Cada passo é uma ida
e volta ao modelo dele: menos passos é o que deixa ele rápido. Um Coucou atualizado com a chave já ligada desliga o
navegador escondido uma vez, ao abrir.

### A velocidade dele

**Velocidade** diz quanto o Hermes pensa antes de cada passo: **Rápido** (menos: responde e age na tela mais cedo, o
melhor para abrir sites, tocar vídeos e clicar), **Normal** (o padrão do Hermes) ou **Caprichado** (mais). O Coucou
manda isso com cada mensagem (`model_options.reasoning_effort` do Hermes: `low`, nada, `high`), então vale a partir da
próxima mensagem, sem reiniciar o Hermes, e as configurações do próprio Hermes ficam como estão.

### O nome e o tom dele (SOUL.md)

Em **O tom dele**, escolha **Amigável**, **Direto**, **Profissional**, **Divertido**, **Mordomo (formal e
espirituoso)** ou **Personalizado** para descrever com as suas palavras (até 1.500 caracteres), e clique em **Salvar**.
Quando o Coucou roda o Hermes, ele grava o nome e o tom no arquivo de personalidade do Hermes, o `SOUL.md` na pasta do
Hermes (*"You are Zheus, the user's personal assistant…"*), e também lembra o tom ao Hermes a cada mensagem: a mudança
vale a partir da sua próxima mensagem.

Um `SOUL.md` que não foi o Coucou que escreveu (o padrão do Hermes, ou o seu) é **copiado antes** para
`SOUL.backup-AAAAMMDD-HHMMSS.md`, ao lado dele. O do Coucou termina com a linha *"Set up in Coucou (Settings → Hermes
Agent)…"*, e o Coucou escreve de novo sempre que o nome ou o tom mudam ali. Se você editar o arquivo, apague essa linha:
assim o Coucou copia a sua versão antes de escrever a próxima. O que você digita como tom fica neste computador, nas
configurações do Coucou e no `SOUL.md` do Hermes, e nunca vai para o log do Coucou.

### Pacotes de skills

- **Casa do Mochi (TVs, luzes…)**: os aparelhos da casa do Mochi, por MCP (seção 6).
- **Busca na web grátis (DuckDuckGo)**: a skill oficial `duckduckgo-search` do Hermes, sem chave.
- **Notícias e feeds (RSS)**: a skill oficial `rss-feeds` do Hermes, sem chave.

O Coucou instala um pacote com o próprio comando do Hermes, `hermes skills install official/research/… --yes` (skills
oficiais que vêm com o Hermes), e, quando você desmarca, remove com `hermes skills uninstall … --yes`, **só se foi o
Coucou que instalou**: uma skill que você instalou por conta própria nunca é removida. Enquanto o Coucou roda o Hermes,
um pacote marcado é instalado na hora; senão, durante a configuração. Um pacote que não instalar aparece nas
Configurações, e o Hermes roda sem ele.

Já vêm com o Hermes, sem nada para instalar: Google Workspace, Notion, mapas, Obsidian, triagem de e-mail e lembretes
(as tarefas agendadas dele).

### Direto no instalador (setup.exe)

O instalador do Windows (`setup.exe`) faz as mesmas perguntas numa página própria, **Seu assistente**, logo depois da
pasta de instalação: **Configurar o assistente agora (recomendado)**, o cérebro (ChatGPT primeiro), a chave de API para
um cérebro que precise de uma (guardada no Gerenciador de Credenciais do Windows, nunca num arquivo), o nome, o tom, os
pacotes de skills e **Deixar meu iPhone alterar o assistente e usar a Casa do Mochi**. A página diz o que vai acontecer
antes do clique: baixar o instalador oficial do Hermes (numa versão fixa, conferida por SHA-256), instalar o Hermes e o
Python dele na sua pasta de usuário, configurar o cérebro, gravar a personalidade no `SOUL.md` (um que já exista ganha
uma cópia de segurança) e instalar as skills marcadas.

O instalador deixa as suas escolhas, nunca a chave, em `%APPDATA%\Coucou\setup.json`. Na primeira vez que abre, o
Coucou lê esse arquivo uma vez, **apaga ele** e começa a mesma configuração do botão Instalar, com as Configurações
abertas em Hermes Agent: com o ChatGPT, é ali que o código aparece. Depois disso não sobra nada para fazer: é só
conversar. Numa reinstalação ou atualização a caixa vem desmarcada, então um Hermes já configurado fica como está.

O instalador MSI e o Linux não têm essa página: use Configurações → Hermes Agent como acima. Tudo o que você escolheu no
`setup.exe` pode ser mudado ali depois, e pelo iPhone pareado quando você deixar.

## Do jeito manual

O resto deste guia é para quem prefere instalar e ligar o Hermes à mão.

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
- **Instalado pelo Coucou**: o instalador é o oficial, fixado num commit e conferido pelo SHA-256 antes de rodar; nada é
  instalado antes do seu clique. O servidor escuta só em `127.0.0.1`. O Hermes recebe só a chave do cérebro que você
  escolheu, nunca as outras chaves do Coucou, e o que ele escreve na saída passa por um filtro que apaga as chaves antes
  de aparecer nas Configurações.

## Limites, com sinceridade

- **A instalação automática** foi testada no que dá para testar sem um Windows de verdade (o instalador conferido, as
  etapas lidas do protocolo do instalador, os comandos de configuração, o ambiente do servidor); a primeira instalação
  real é a sua. Se uma etapa falhar, a tela mostra qual e a última linha que o instalador escreveu.
- **Testado com um Hermes simulado**: as tarefas do servidor de API, os eventos, as aprovações e o parar, e um cliente
  MCP, ainda não com um Hermes de verdade. A API do Hermes é documentada e versionada (`/v1/capabilities`); o Coucou
  recusa um servidor que não é o Hermes ou que é antigo demais.
- **O Hermes no mesmo computador** (ou acessível por https). A chave nunca vai sem criptografia para outra máquina.
- **A qualidade das respostas** é a do modelo que você escolheu no Hermes.
- O plugin do Hermes em **Configurações → Agentes** (as sessões do Hermes na pílula dele) continua funcionando e pode
  ficar ligado junto.

## Para desenvolvedores

Veja a seção "For developers" de [HERMES.md](HERMES.md).
