# Casa do Mochi (Windows e Linux)

*English: [SMART_HOME.md](SMART_HOME.md)*

"Mochi, liga a TV da sala e abre o YouTube." Com a **Casa do Mochi**, o chat do Coucou para Windows e Linux (e o chat
do celular pareado) passa a usar os aparelhos da sua casa: ligar ou desligar uma TV, mudar o volume, abrir um app da
TV, trocar de canal, diminuir uma luz, ligar uma tomada, rodar uma rotina como "Boa noite". Depois de cada ação o
Mochi pergunta de novo ao aparelho e conta o que ele responde ("a TV está ligada, o YouTube está na tela"), e não o
que ele esperava que acontecesse.

Três caminhos, todos só na sua rede local:

- **Home Assistant**, pela API REST oficial, com um token de acesso de longa duração: media players (TVs, caixas de
  som), luzes e interruptores (tomadas, relés), aspiradores robô (iniciar, pausar, voltar para a base, e cômodo por
  cômodo com o `vacuum.clean_area` do Home Assistant 2026.3), ar-condicionado e termostatos (ligar, desligar, uma
  temperatura) e sensores de temperatura e umidade, só para leitura. As pessoas dele (`person`) são lidas para saber
  quem está em casa, para o [Zheus One](ZHEUS_ONE.pt-BR.md), nunca acionadas.
- **TVs Samsung** (2016 em diante, Tizen), direto na TV: a API de controle remoto que os apps de celular da Samsung
  usam, nas portas 8001 e 8002. A TV pergunta uma vez na tela se pode permitir o Coucou.
- **TVs Philips**, direto na TV, pelo JointSpace, a API que as TVs Philips respondem nas portas 1925 e 1926. As
  Android TVs são pareadas uma vez com um PIN que aparece na tela.
- **TVs Android, projetores e TV boxes**, direto no aparelho pelo ADB, a depuração pela rede do Android (porta 5555),
  depois de ligada nas Opções do desenvolvedor do aparelho. O aparelho pergunta uma vez na tela se pode permitir este
  computador.

Uma TV de outro tipo não é adicionada: o Coucou diz por onde ela responde, para o próximo passo ser escolhido com
base em fatos.

Vem **desligada**. Enquanto está desligada, nada é lido e nada é enviado.

## O que você precisa

- O Coucou para Windows ou Linux com os **Braços do Mochi** (a versão depois da 0.4.0, ainda não lançada: pegue o
  instalador nos artefatos do workflow **Windows installer**, ou compile a partir de [`windows/`](../windows/README.md)).
- Um modelo de chat que usa ferramentas: Anthropic, Google, OpenAI, OpenRouter, ou um modelo local com suporte a
  chamadas de ferramenta.
- O computador na mesma rede que o Home Assistant e as TVs.

## Configurando

1. **Configurações → Braços do Mochi**: ligue e escolha uma pasta de trabalho. A casa é usada pelos braços dele.
2. **Configurações → Casa do Mochi**: ligue a **Casa do Mochi**.
3. **Procurar na rede** procura por alguns segundos (mDNS e UPnP). O Home Assistant e as TVs encontradas vêm com
   **Usar** ou **Adicionar**: uma TV de outro fabricante (o app de espelhar tela de um projetor Android, por exemplo) é
   testada pelo ADB quando você adiciona, e senão o Coucou diz por onde ela responde. Nada é adicionado sem o seu
   clique.
4. **Home Assistant**: o endereço (por exemplo `http://homeassistant.local:8123`) e um **token de acesso de longa
   duração** (no Home Assistant: seu perfil → Segurança → Tokens de acesso de longa duração), depois **Conectar**. O
   Coucou mostra a versão que respondeu. Os cômodos vêm das áreas do Home Assistant quando o token é de um
   administrador; senão, defina os cômodos no Coucou.
5. **TVs**: o endereço da TV (nas configurações de rede da TV) e **Adicionar**, ou **Adicionar** ao lado de uma TV
   que a busca encontrou. O Coucou pergunta primeiro se ela é uma Samsung, depois se é uma Philips:
   - **Samsung**: a API dela diz o nome e o modelo ("UN43AU7700GXZD · Tizen · API 2.0.25 · criptografada (porta
     8002)") e o endereço MAC do Wi-Fi; o renderizador UPnP dela, quando existe, dá o volume em número e o mudo
     ("volume por UPnP"). Toque em **Parear**: a TV pergunta na tela se pode permitir o Coucou; escolha **Permitir**
     com o controle remoto (você tem meio minuto). A TV então dá ao Coucou um token, guardado no cofre do sistema, e
     não pergunta mais.
   - **Philips**: o Coucou pergunta qual versão do JointSpace ela fala (6, 5 ou 1), por HTTPS na porta 1926 ou HTTP
     na 1925, e o que ela sabe fazer, e mostra: "55PUS8007/12 · JointSpace 6.4.0 · Android TV · criptografada (porta
     1926)". Uma Android TV precisa de **Parear**: ela mostra um PIN, digite em **PIN da TV** e **Confirmar**.
   - **Android pelo ADB** (quando não é nenhuma das duas): veja *TVs Android, projetores e TV boxes* abaixo.
   - **Nenhuma delas**: o aparelho não é adicionado, e a resposta lista por onde ele responde ("DIAL, UPnP
     MediaRenderer…"), com o jeito de ligar a depuração pela rede de um aparelho Android. Mande essa linha para quem te
     ajuda com o Coucou: ela diz qual caminho, se houver, poderia controlá-lo.

   O **endereço MAC** (para ligar a TV do standby profundo com Wake-on-LAN) vem da TV ou da tabela de vizinhos do
   computador; você também pode digitá-lo.
6. **Aparelhos → Ler os aparelhos**: cada aparelho com o que ele informou que sabe fazer e o estado atual. Marque **O
   Mochi pode usar** nos que ele pode usar (os do Home Assistant começam desmarcados; uma TV que você adicionou começa
   marcada), dê um nome e um cômodo ("TV", "Sala"), e marque **Perguntar antes de agir** se ele deve sempre esperar o
   seu clique. **Testar** executa qualquer ação que o aparelho informou, com um valor quando ela pede (porcentagem,
   app, canal).
7. **Rotinas**: **Sugestões** oferece "Boa noite" (TVs, caixas de som e luzes desligadas), "Modo cinema" (a TV da
   sala ligada e as luzes dela baixas) e "Ligar …" para cada TV com cômodo, feitas com os seus próprios aparelhos.
   **Editar e salvar** para guardar uma; **Nova rotina** para escrever a sua (até 20 passos, cada um com uma espera
   opcional de até 120 s, para uma TV que precisa de um tempo antes de abrir um app).
8. **Voz** (opcional): **Ler as respostas do Mochi em voz alta**, e **Testar** para ouvir a voz. Veja *Voz* abaixo.

### TVs Android, projetores e TV boxes (ADB)

Muitos aparelhos Android sem a certificação de TV do Google (projetores como o HY300, TV boxes) só podem ser usados
pelo ADB, a ponte de depuração do Android. O próprio Coucou fala esse protocolo, na porta 5555 (nada para instalar):

1. No aparelho, abra **Configurações → Sobre** (em alguns projetores, "Sobre nós") e toque sete vezes em **Número da
   versão**: aparecem as **Opções do desenvolvedor**. Ligue lá a **Depuração USB**, e a **depuração pela rede** (ou
   "ADB pela rede") quando ela estiver na lista.
2. No Coucou, digite o endereço do aparelho (nas configurações de rede dele) em **TVs e projetores** e toque em
   **Adicionar**. O Coucou tenta primeiro os caminhos da Samsung e da Philips, depois bate na porta 5555 sem oferecer
   nada.
3. Toque em **Parear**. O aparelho mostra **Permitir depuração USB?** com a impressão digital da chave deste
   computador: marque **Sempre permitir neste computador** e escolha **Permitir** (você tem um minuto). O Coucou então
   lê o que o aparelho é (modelo, versão do Android e nome), o que ele informa (volume, mudo) e os apps que a tela
   inicial dele mostra.

A chave é RSA-2048, como o adb faz, criada pelo Coucou na primeira vez e guardada no cofre do sistema (nunca em
arquivo). O ADB dá controle total do aparelho, por isso o Coucou só manda os próprios comandos fixos: teclas (acordar,
dormir, volume, mudo), abrir um app da lista do próprio aparelho e ler o estado dele (`dumpsys`). Nada que o modelo
escreve vira comando. A **depuração sem fio** do Android 11 (com código de pareamento) é outro protocolo, com TLS, que
o Coucou não fala: ele avisa, e o caminho é a depuração pela rede clássica.

O que funciona depois: ligar a partir do modo de espera e desligar (para o modo de espera), volume para cima e para
baixo e em número, mudo, abrir um app, e ler se ele está ligado, o volume e o app na tela. Um aparelho que desliga a
rede quando é desligado (a maioria dos projetores e TV boxes) não pode ser ligado pelo Coucou: ligue pelo controle
remoto.

## Falando com o Mochi

Peça no chat como pediria para alguém em casa:

- "Mochi, liga a TV da sala e abre o YouTube": a TV é ligada, o Mochi espera até ela informar que ligou, abre o
  YouTube pela lista de apps da própria TV e diz o que está na tela.
- "Abaixa o volume da TV do quarto para 15", "Põe no canal 5", "Apaga a luz da cozinha".
- "Boa noite" roda a rotina com esse nome.

Os aparelhos são encontrados pelo nome e pelo cômodo ("TV da sala", "TV do quarto"); apps e canais, pelas listas do
próprio aparelho. Quando algo é ambíguo ou não é possível, o Mochi diz isso em vez de chutar.

## Voz

- **No computador** (Windows 10 e 11): abra o chat e aperte **Windows + H**. A digitação por voz do Windows escreve o
  que você diz no campo do chat, no idioma do teclado em uso (Windows + Espaço troca), depois aperte Enter. O Linux
  não tem digitação por voz própria: fale pelo celular.
- **Ler as respostas em voz alta** (Configurações → Casa do Mochi → Voz, vem desligado): as respostas ao que você
  perguntou no chat da ilha são lidas com as vozes do próprio computador, no idioma do app. Só vozes que rodam no
  computador são usadas, então nada é enviado para lugar nenhum. Para português do Brasil, o Windows tem a "Microsoft
  Maria"; adicione vozes em Configurações do Windows → Hora e idioma → Fala. Silenciar os sons da ilha também silencia
  a leitura.
- **No iPhone** (versão sideload, chat com o computador): o microfone ao lado do campo de texto. Fale, faça uma pausa,
  e a mensagem é enviada; a resposta é lida em voz alta com as vozes do iPhone (o menu ⋯ do chat desliga isso e
  escolhe o idioma do ditado). O reconhecimento roda no próprio iPhone quando ele consegue.

## Segurança

- **Nada que o modelo escreve é executado direto.** O Mochi tem três ferramentas: listar os aparelhos, controlar um,
  rodar uma rotina. Cada ação vira uma de um conjunto fixo (ligar/desligar, volume, mudo, abrir app, canal, brilho),
  é conferida contra o que aquele aparelho informou, e vira uma chamada de uma lista fixa: no Home Assistant, só
  serviços de `media_player`, `light`, `switch`, `vacuum` (`start`, `clean_area`, `pause`, `return_to_base`) e
  `climate` (`turn_on`, `turn_off`, `set_temperature`) como `turn_on`, `volume_set` ou `select_source`. Scripts,
  automações, comandos de shell, o `send_command` cru do aspirador, fechaduras, alarmes ou reinicializações não podem
  ser alcançados.
- **Só os aparelhos que você marcou.** O Mochi nem vê os outros.
- **Clique para o que importa.** Tomadas e interruptores sempre esperam um clique (Permitir na ilha, ou no celular com
  Face ID), assim como os aparelhos marcados com **Perguntar antes de agir**; **Perguntar antes de toda ação** faz
  todos esperarem. Com os Braços do Mochi em modo só leitura, as ações na casa são recusadas.
- **Credenciais no cofre do sistema** (Gerenciador de Credenciais do Windows, ou o Secret Service no Linux): o token
  do Home Assistant e os pareamentos das TVs. Nunca num arquivo, nunca nos logs, nunca enviados ao provedor de IA.
- **Só a rede local.** Cada endereço é conferido antes de cada requisição: um IP precisa ser privado, link-local ou
  loopback, e um nome precisa ser local (`.local`, `.lan`, `.home.arpa`, uma palavra só) ou apontar só para esses
  endereços. Um endereço público é recusado, e essas requisições nunca passam por um proxy.
- **O certificado da TV fica fixado.** As TVs Samsung e Philips usam um certificado autoassinado: o Coucou guarda o
  SHA-256 dele no primeiro contato e recusa um diferente depois (remova a TV e adicione de novo se você sabe por que
  ele mudou).
- **Um registro.** Cada ação, quem pediu (chat, celular, rotina, Configurações) e como foi fica em `home-log.jsonl`
  (`%LOCALAPPDATA%\Coucou` no Windows, `~/.local/share/coucou` no Linux), no máximo 1.000 linhas, só neste
  computador. **Mostrar as últimas ações** nas Configurações lê esse registro.
- O Home Assistant em `http://` simples manda o token sem criptografia pela sua rede: use `https://` se puder.
- **Outros agentes** (o Hermes Agent, ou outro cliente MCP deste computador) só chegam à casa depois que você liga
  **Configurações → Hermes Agent → Casa do Mochi para o Hermes**, com as mesmas regras, e o registro mostra o nome
  deles. Veja [HERMES.pt-BR.md](HERMES.pt-BR.md).

## Limites, com sinceridade

- **Testado com aparelhos simulados.** O código é conferido contra um Home Assistant simulado, uma TV Samsung
  simulada (a API, o controle remoto por TLS com a pergunta "Permitir" e o token, os apps, o volume por UPnP) e uma TV
  Philips simulada (requisições, pareamento, autenticação digest, erros), ainda não contra TVs reais. Nem a API de
  controle remoto da Samsung nem o JointSpace são APIs publicadas, e as duas mudam entre anos e firmwares. O Coucou só
  oferece o que a sua TV responde; se algo falhar, as Configurações e o Mochi dizem o que a TV respondeu.
- **Ligar a TV.** Uma TV em standby pode continuar respondendo na rede (aí ela é ligada pela API ou pela tecla de
  standby) ou não responder nada: aí só o Wake-on-LAN acorda, o que precisa do endereço MAC, do computador no mesmo
  segmento de rede e da TV configurada para aceitar ser ligada pela rede (Samsung: "Ligar com dispositivo móvel", em
  Configurações → Geral → Rede → Configurações especialistas; Philips: "Ligar com Wi-Fi (WoWLAN)" ou "Ligar com
  Chromecast"; nos modelos que têm essas opções, e o nome exato muda com o idioma da TV). Uma TV no Wi-Fi em standby
  profundo pode não acordar. O Coucou espera até 25 s e diz "não confirmou" em vez de fingir que deu certo.
- **TVs Samsung**: a lista de apps vem da TV, e nem todo modelo a fornece (aí nenhum app é oferecido); que o app
  abriu só é confirmado nas TVs que dizem qual app está na tela. Canais: só próximo e anterior (a TV não dá a lista de
  canais). Volume em número e mudo precisam do renderizador UPnP; sem ele, só aumentar e diminuir. Samsung de antes de
  2016 (outro protocolo) não é suportada.
- **Apps** abrem nas Android TVs que listam os apps; **canais**, só nas TVs que listam os canais. O app ou o canal
  precisa estar na lista da própria TV.
- **Home Assistant**: media players, luzes, interruptores, aspiradores robô, ar-condicionado e sensores de
  temperatura e umidade. Cômodos vindos das áreas precisam de um token de administrador (a API de templates); senão,
  defina os cômodos no Coucou. Canais não são oferecidos pelo Home Assistant. O aspirador limpa cômodo por cômodo só
  com o Home Assistant 2026.3 ou mais novo e uma integração que suporte isso (ela informa `CLEAN_AREA`); a bateria
  dele vem do próprio sensor de bateria. As temperaturas do ar-condicionado são lidas em °C.
- **Outras marcas** (LG, Sony, TCL…) aparecem na busca mas não são controladas direto: adicione-as ao Home
  Assistant, que tem integrações para muitas delas, e elas aparecem como media players dele. Adicionar uma pelo
  endereço diz por onde ela responde, para um suporte futuro.
- **Android pelo ADB** foi testado com um aparelho simulado (a conexão, a assinatura da chave conferida como o Android
  confere, a pergunta na tela, comandos e o que eles imprimem) e com as saídas do Android 4.4, 7 a 9 e 13, ainda não
  com um projetor de verdade. Os apps recebem o nome pelo pacote (uma lista de apps conhecidos, senão a palavra do
  próprio pacote: "com.waxrain.airplaydmr" vira "Waxrain"); o volume em número anda nos passos do aparelho (15 na
  maioria). Se o aparelho mantém a depuração pela rede ligada depois de reiniciar depende do firmware dele. Um aparelho
  que desliga o Wi-Fi junto com ele não pode ser ligado pela rede.
- **A busca** usa mDNS e SSDP, que redes de convidados, VPNs ou alguns roteadores bloqueiam: adicione pelo endereço.
- **A digitação por voz no computador** é a do Windows; o Linux não tem. A leitura em voz alta precisa de uma voz do
  idioma do app instalada no computador.

## Para desenvolvedores

- `windows/src-tauri/src/smart_home/`: `model.rs` (aparelhos, capacidades, ações, as conferências),
  `home_assistant.rs`, `samsung.rs` (o controle remoto da Samsung: a API, o WebSocket com a pergunta "Permitir" e o
  token, os apps), `upnp.rs` (volume e mudo de um renderizador), `philips.rs` (JointSpace, pareamento, Wake-on-LAN, o
  TLS com certificado fixado usado pelas duas marcas), `digest.rs` (HTTP Digest, MD5 e SHA-256,
  com os vetores de teste das RFCs), `discovery.rs` (mDNS e SSDP), `android.rs` (ADB: o protocolo, a chave RSA que o
  Android pede, o formato da chave pública dele, os comandos fixos e a leitura do que eles imprimem), `routines.rs`,
  `journal.rs`, `config.rs` (configurações e cofre), `mod.rs` (leitura da casa, execução de uma ação, comandos das
  Configurações). As dependências são as que o app já usa (reqwest com rustls, ring, mdns-sd, tokio,
  tokio-tungstenite), e o `num-bigint` para a chave RSA do ADB, que já estava no lock file pelo cofre do Linux.
- `windows/src-tauri/src/agent/tools.rs`: as ferramentas `home_devices`, `home_control` e `home_routine`; `policy.rs`
  decide qual ação espera um clique.
- `windows/src/settings/home.ts` e `windows/src/core/home.ts`: Configurações → Casa do Mochi. `windows/src/core/voice.ts`:
  a leitura das respostas em voz alta.
- Testes: `cargo test --workspace` (Home Assistant e TVs Samsung e Philips simulados, um turno de ponta a ponta
  "liga a TV, depois abre o YouTube", uma tomada negada nunca tocada) e `npm test` (`tests/home.test.mjs`,
  `tests/voice.test.mjs`).
