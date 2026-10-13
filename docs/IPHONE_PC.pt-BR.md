# Coucou: os braços do Mochi no Windows e o iPhone ligado ao PC

Guia em português para duas novidades desta branch:

1. **Os braços do Mochi** (Windows e Linux): o chat da ilha deixa de só conversar e passa a **agir no computador**,
   como o Codex: olha suas pastas, lê e altera arquivos, roda comandos, abre e lê páginas da web — com qualquer IA que
   o chat estiver usando (Anthropic, OpenAI, Google, OpenRouter, Ollama, LM Studio).
2. **O iPhone ligado a um PC Windows ou Linux**: antes o app do iPhone só seguia um Mac (via iCloud). Agora ele
   também se conecta direto ao Coucou do PC, pela rede local, com o mesmo protocolo do app Android. Com
   **notificações mesmo com o Coucou fechado** (pelo app ntfy) e **voz** no chat.
3. **A Casa do Mochi**: o chat passa a usar os aparelhos da sua casa (Home Assistant e TVs Philips): *"Mochi, liga a
   TV da sala e abre o YouTube"*. Guia próprio: [SMART_HOME.pt-BR.md](SMART_HOME.pt-BR.md).

## 1. Os braços do Mochi no PC

### Ligar

1. **Configurações → Braços do Mochi** → ligue a chave. Vem desligado.
2. **Pasta de trabalho**: a pasta onde o Mochi pode olhar sem pedir (por exemplo a pasta dos seus projetos). Escreva o
   caminho completo e clique em **Usar esta pasta**. Se deixar vazio, ele sugere a pasta Documentos.
3. **Sem perguntar** — o quanto ele pode fazer sozinho:
   - **Perguntar antes de cada alteração** (padrão): toda alteração, comando e acesso à web espera o seu clique;
   - **Editar arquivos da pasta sem perguntar**: alterações *dentro da pasta* são feitas direto; comandos e web
     continuam pedindo;
   - **Somente leitura**: ele só olha, nunca altera nada nem roda comandos.

### Usar

- No chat da ilha aparece um **botão de mão** ao lado do nome do modelo. Aceso = o Mochi pode agir neste chat;
  apagado = conversa normal.
- Peça normalmente: *"liste o que tem na pasta do projeto"*, *"rode os testes e me diga o que falhou"*, *"troque o
  título do README para …"*.
- Cada passo aparece no chat enquanto ele trabalha (Lê, Edita, Executa…). Clique num comando para ver a saída.
- Quando ele quer mudar algo, aparece um **cartão com Permitir / Negar** (com o diff, no caso de arquivo). Nada é
  alterado sem o seu clique (a não ser o que você liberou em "Sem perguntar").
- Enquanto ele trabalha, o botão de enviar vira **Parar**: o comando em execução é encerrado (com todos os processos
  filhos).

### O que ele nunca faz sozinho

- Comandos (PowerShell no Windows, bash no Linux), acessar ou abrir páginas da web: **sempre pedem clique**, em
  qualquer nível.
- Olhar **fora da pasta de trabalho** ou arquivos que parecem guardar **chaves ou senhas** (`.env`, `.ssh`, `id_rsa`,
  `*.pem`, cofres de senha, dados do navegador…): **sempre pedem clique**. Uma busca de texto nunca abre esses
  arquivos.
- Um arquivo que mudou depois que você viu o cartão não é sobrescrito: ele recusa e avisa.
- O que o Mochi lê vai para o provedor de IA do chat, como tudo o que você digita.

## 2. O iPhone ligado ao PC

### No PC

1. **Configurações → Celular (Android e iPhone)** → ligue **Link com o celular**. O iPhone precisa estar no mesmo
   Wi-Fi. Se o Windows perguntar sobre o firewall, permita o Coucou em **redes privadas**.
2. Opcionais (cada um desligado até você ligar):
   - **Mostrar os detalhes da sessão no celular** (passos, última linha, nome da pasta);
   - **Deixar o celular responder às perguntas do Claude Code**;
   - **Permitir que o celular converse com meus provedores de IA** → **Escolher modelos** e marque os que o celular
     pode usar (as chaves continuam só no PC);
   - **Deixar o celular usar os braços do Mochi** (precisa do chat acima e dos Braços do Mochi ligados).
3. **Mostrar código de pareamento**.

### Instalar o app no iPhone sem Mac

O app oficial (TestFlight/App Store) é do autor. Sem Mac e sem conta paga de desenvolvedor, use a **versão
sideload**, que o GitHub Actions compila para você:

1. No GitHub do seu repositório: **Actions → iPhone (sideload)**. Se for um fork, ative o Actions uma vez e clique em
   **Run workflow**. Quando ficar verde, baixe o artefato **Coucou-iPhone-sideload** (um zip com o
   `Coucou-iPhone-sideload.ipa`).
2. No Windows, instale o [Sideloadly](https://sideloadly.io) (ou o [AltStore](https://altstore.io) com o AltServer),
   ligue o iPhone no cabo, arraste o `.ipa` e entre com o seu Apple ID (conta gratuita serve).
3. No iPhone: **Ajustes → Geral → VPN e Gerenciamento de Dispositivos** → confie no app do seu Apple ID. No iOS 16 ou
   mais novo: **Ajustes → Privacidade e Segurança → Modo de Desenvolvedor** → ligar (o iPhone reinicia).
4. Com conta gratuita o app para de abrir depois de **7 dias**: é só instalar de novo pelo Sideloadly (ou deixar o
   AltStore renovar sozinho, com o AltServer aberto no PC e o iPhone no mesmo Wi-Fi).

Essa versão não tem iCloud nem push da Apple (conta gratuita não tem): ela fala só com o PC. Os widgets ficam vazios e
a Live Activity do Mac não se aplica; o resto funciona.

### Parear

1. Abra o **Coucou no iPhone → ⚙︎ → Windows or Linux computer → Scan the code** (na primeira abertura, o botão
   **Scan the pairing code**) e aponte para o QR code do PC. Use o leitor do próprio Coucou: a **Câmera do iPhone não
   abre códigos do Coucou** e responde "No usable data found". Outra opção: no PC, **Copiar link**, mande para o
   iPhone (WhatsApp, e-mail…) e toque no link, ou cole em **Coucou → ⚙︎ → Windows or Linux computer**.
2. O Coucou mostra o nome do PC: toque em **Pair**. Quando o iOS pedir, permita o acesso à **rede local**.
3. As sessões do PC aparecem em **Agents**. O botão 🖥 no canto superior esquerdo abre o **chat com o Mochi do PC**;
   no menu ⋯ você escolhe o modelo e liga ou desliga os braços dele.

### Notificações com o Coucou fechado (ntfy)

O iOS pausa o Coucou uns 30 segundos depois que você sai dele, e só o push da Apple poderia acordá-lo (a versão
sideload, com conta gratuita, não tem). Para não perder pedidos, o PC pode avisar pelo **ntfy**, um app de
notificações gratuito:

1. No iPhone, instale o app **ntfy** (App Store) e permita as notificações dele.
2. No PC: **Configurações → Celular (Android e iPhone) → Notificações com o Coucou fechado** → ligue. Aparece um
   **Tópico** (um nome aleatório, difícil de adivinhar): **Copiar**.
3. No ntfy do iPhone: toque em **+**, cole o tópico, deixe o servidor padrão (ntfy.sh) e assine.
4. No PC: **Enviar notificação de teste**. Ela deve chegar no iPhone em alguns segundos.

Como funciona: quando um pedido (aprovação ou pergunta) fica **8 segundos** esperando e **nenhum celular está
conectado**, o PC manda um aviso pelo ntfy, por exemplo *"Claude Code está esperando seu OK"*. Tocar no aviso abre o
Coucou, que reconecta e mostra o pedido para você aprovar com Face ID. No máximo um aviso a cada 20 segundos; um
pedido respondido no PC antes disso não é avisado.

Privacidade: o aviso passa pelo servidor do ntfy e diz só **quem** está esperando, nunca o comando, um caminho, o
texto da pergunta ou o nome do computador. Quem souber o tópico consegue ler esses avisos: **Novo tópico** troca o
nome (aí assine o novo no app). Prefere o seu próprio servidor ntfy? Coloque o endereço em **Servidor** (https, ou
http só na rede local) e, se ele pedir, um **Token de acesso (opcional)**, guardado no cofre do sistema. Vem
desligado.

### Falar com o Mochi pelo iPhone

No chat com o computador (botão 🖥), toque no **microfone** ao lado do campo de texto e fale: *"liga a TV da sala e
abre o YouTube"*. Depois de uma pausa curta a mensagem é enviada sozinha, e a resposta é **lida em voz alta** com as
vozes do iPhone. No menu ⋯ do chat: **Read answers to spoken messages aloud** (liga ou desliga a leitura) e
**Dictation language** (o idioma do ditado, por exemplo Português (Brasil)). O reconhecimento de voz roda no próprio
iPhone quando ele consegue. No PC, aperte **Windows + H** no chat da ilha para ditar (detalhes em
[SMART_HOME.pt-BR.md](SMART_HOME.pt-BR.md#voz)).

### No dia a dia

- **Permitir** pede **Face ID**; **Negar** não. Responder a uma pergunta também pede Face ID.
- Com os braços ligados, peça pelo iPhone: *"roda os testes do projeto"*. Os passos aparecem na hora; o que alterar
  algo chega como pedido de aprovação, no iPhone e na ilha do PC — vale o primeiro clique.
- Um comando com mais de 500 caracteres só pode ser **negado** pelo iPhone (o resto não chegou até ele): aprove no PC.

### Limites (sem enrolação)

- **Só na rede local**: mesmo Wi-Fi (ou o hotspot do próprio PC). Rede de convidados com isolamento, VPN ou firewall
  bloqueando a porta impedem a conexão. Se o IP do PC mudar, o iPhone o encontra de novo sozinho (e confere o
  certificado antes de confiar).
- **O iPhone não acorda sozinho**: o iOS pausa o app pouco depois que ele sai da tela, e só o push da Apple poderia
  acordá-lo (isso exige servidor e conta paga). Então os pedidos chegam com o Coucou aberto e nos ~30 segundos depois
  de sair dele (como notificação, que dá para responder na tela bloqueada); depois disso, só com as **notificações
  pelo ntfy** ligadas (acima), que avisam e abrem o Coucou com um toque. Abriu o app, ele reconecta na hora. O cartão
  no PC sempre funciona e nunca fica esperando o celular.
- **Um código, um celular**: **Parear de novo** no PC gera outro código e desconecta o celular antigo.

### Segurança

- O iPhone só aceita **um certificado**: o do QR code (SHA-256 fixado), em TLS 1.2 ou mais novo.
- O token do pareamento fica no **Keychain** do iPhone, só neste aparelho (fora de backups e do iCloud).
- Pelo link passam: estados e passos das sessões, o comando de cada pedido, as perguntas e suas escolhas, e as
  mensagens do chat. **Nunca** as chaves de API do PC ou arquivos. Nos passos e pedidos do Mochi, um caminho aparece
  a partir da pasta de trabalho (`notas\todo.txt`), ou a partir de `~` no resto da sua pasta de usuário
  (`~\Downloads\a.txt`), então nenhum mostra o seu nome de usuário.

## Instalar esta versão no PC

Não precisa de VS Code nem de compilar nada: o GitHub Actions gera o instalador.

1. No GitHub: **Actions → Windows installer**. Ele roda sozinho quando o app do Windows muda; para rodar na hora,
   **Run workflow**. Quando ficar verde (uns 15–25 minutos), abra a execução e, em **Artifacts**, baixe
   **Coucou-Windows-msi** (um zip com o `.msi` dentro). O `-setup.exe` também está lá, mas o `.msi` costuma passar
   melhor pelo Windows Defender.
2. Desinstale o Coucou antigo: **Configurações do Windows → Aplicativos → Aplicativos instalados → Coucou →
   Desinstalar**. Suas configurações e chaves de API ficam guardadas (Credential Manager) e voltam no novo.
3. Extraia o zip e abra o `.msi`. O instalador não é assinado: se aparecer "O Windows protegeu o computador", clique
   em **Mais informações → Executar assim mesmo**.
4. Abra o Coucou e ligue **Configurações → Braços do Mochi** (e, se quiser, **Configurações → Casa do Mochi**).

Sem instalar nada, também dá para usar o **Coucou-Windows-portable**: extraia e rode o `coucou.exe` (deixe o
`coucou-hook.exe` na mesma pasta).

Para desenvolver (opcional), rode a partir do código, num terminal (PowerShell serve, VS Code é opcional); detalhes em
[windows/README.md](../windows/README.md#build-it-yourself):

```
cd windows
npm install
npm run tauri dev      # ou: npm run tauri build  (gera o instalador)
```

Precisa de Node 22.18 ou mais novo, Rust, e no Windows as Build Tools do Visual Studio (C++) e o WebView2.

Detalhes técnicos em inglês: [IPHONE_PC.md](IPHONE_PC.md) e [ANDROID_LINK.md](ANDROID_LINK.md) (protocolo,
capacidade `agent`).
