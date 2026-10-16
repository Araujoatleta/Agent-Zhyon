# Frota Coucou controlada por um VPS

## O que esta arquitetura entrega

Você instala o Coucou em qualquer computador Windows, executa o pacote daquele computador uma vez e ele passa a abrir uma
conexão **de saída** para seu VPS. Não é necessário abrir porta no roteador, usar IP fixo em casa ou deixar um terminal
aberto. O VPS só aproxima o celular do computador escolhido; arquivos, aplicativos, Claude, Codex e os Braços do Mochi
continuam rodando no computador que será controlado.

```text
celular
   │  sala e chave de pareamento exclusivas (criptografia ponta a ponta)
   ▼
seu domínio estável ── HTTPS/WSS ── VPS
                                  │
                                  ├── relay opaco ── PC escritório (chave revogável A)
                                  ├── relay opaco ── notebook     (chave revogável B)
                                  └── relay opaco ── PC viagem    (chave revogável C)
```

Cada computador recebe:

- uma chave de acesso própria, usada apenas para entrar no relay e revogável sem afetar os outros computadores;
- uma sala aleatória por pareamento;
- uma chave ponta a ponta que nunca chega ao VPS;
- o Coucou e suas credenciais de agentes somente no Windows daquele computador.

O relay não possui acesso remoto genérico ao Windows. Ele encaminha os frames criptografados do protocolo Coucou. Isso
evita transformar o VPS em uma senha-mestra de todos os computadores.

## Instalação nova do VPS em um comando

Pré-requisitos: Ubuntu/Debian limpo, acesso root, portas 80 e 443 liberadas e um registro DNS apontando para o VPS.

```sh
git clone https://github.com/Araujoatleta/Agent-Zhyon.git
cd Agent-Zhyon/android/relay
sudo sh deploy/install-vps.sh coucou-relay.seudominio.com
```

O instalador coloca Docker e Compose quando necessário, cria `.env`, gera a primeira chave, inicia o relay e inicia Caddy.
Caddy emite e renova o certificado TLS automaticamente. O serviço reinicia sozinho após reboot.

Se o VPS já possui Nginx/Caddy e outras aplicações, copie `.env.example` para `.env`, use
`COUCO_MODE=existing-proxy`, execute `./vpsctl up` e encaminhe somente `/` e `/v1/room/*` para
`127.0.0.1:8787`. O modelo está em `deploy/nginx-coucou-relay.conf`.

## Adicionar um computador Windows

No VPS:

```sh
cd Agent-Zhyon/android/relay
./vpsctl issue escritorio
./vpsctl client-package escritorio
```

Baixe `dist/Coucou-escritorio.zip` somente para aquele computador, extraia e dê dois cliques em `install.cmd`. O pacote:

1. baixa o instalador oficial da release `windows-latest` por HTTPS;
2. instala o Coucou para o usuário atual;
3. preserva um backup das configurações existentes;
4. grava o endereço do VPS e a chave exclusiva no Gerenciador de Credenciais do Windows;
5. liga o relay, abre o Coucou e confirma que o processo permaneceu rodando;
6. apaga o `profile.json` extraído ao terminar.

O ZIP contém uma credencial revogável. Apague o ZIP e a pasta extraída depois da instalação. Para cadastrar outro
computador, use outro nome e gere outro pacote. Nunca reutilize um pacote.

No Coucou do computador, abra **Configurações → Celular → Mostrar código de pareamento** e escaneie o código no celular.
O ato de escanear continua intencionalmente manual: é ele que entrega a chave ponta a ponta sem passar pelo VPS.

## Operação diária

```sh
./vpsctl list                    # nomes cadastrados; nunca mostra chaves
./vpsctl status                  # containers e saúde
./vpsctl doctor                  # DNS + TLS + proxy + relay
./vpsctl logs                    # últimos logs operacionais, sem conteúdo de sessões
./vpsctl revoke escritorio       # corta somente esse computador
./vpsctl issue escritorio-novo   # cria uma identidade nova
```

O relay aceita até mil chaves de computador por instalação. As salas continuam efêmeras e desaparecem da memória quando
os dois lados saem.

## Trocar de VPS levando uma pasta

Mantenha o mesmo domínio público. No VPS antigo:

```sh
./vpsctl backup
```

O arquivo criado em `dist/` contém o programa, `.env`, chaves dos computadores e, no modo portátil, o estado do Caddy.
Ele deve ser tratado como uma senha: transporte e armazene de forma privada e criptografada.

No VPS novo:

1. instale Docker e Docker Compose;
2. extraia o `.tar.gz` em uma pasta;
3. atualize o DNS do mesmo domínio para o IP novo;
4. nessa pasta, execute `./vpsctl up` e depois `./vpsctl doctor`.

Os computadores não precisam ser reconfigurados porque continuam usando o mesmo domínio e suas próprias chaves. Caddy
reutiliza ou reemite o certificado automaticamente. Se você copiar a própria pasta com `rsync`/SFTP em vez do backup, o
resultado é o mesmo: tudo que pertence à frota fica abaixo de `android/relay/`.

## O que fica dentro da pasta portátil

```text
android/relay/
├── compose.portable.yml       relay + TLS automático para VPS dedicado
├── compose.vps.yml            relay privado para VPS que já tem proxy
├── deploy/                    Caddy, Nginx e instaladores
├── state/
│   ├── clients/*.key          uma chave revogável por computador
│   └── caddy/                 certificados e estado do TLS
├── .env                       domínio e modo da instalação
└── vpsctl                     instalar, cadastrar, revogar, testar e fazer backup
```

`state/`, `.env`, `dist/` e segredos nunca são enviados ao GitHub.

## Segurança e limites reais

- A conexão do computador é somente de saída. Não habilite UPnP e não publique a porta 47821.
- O relay vê IP, horário, sala e tamanho acolchoado dos frames; não vê comandos, arquivos, respostas nem chaves de
  pareamento.
- As confirmações do Coucou continuam valendo. Aprovação ausente expira e nunca vira permissão automática.
- Perdeu um celular: escolha **Parear de novo** no computador para invalidar a sala e a chave ponta a ponta antigas.
- Perdeu um computador/pacote: `./vpsctl revoke NOME`; os outros computadores continuam conectando.
- O Android implementa o transporte relay. O cliente iPhone desta branch ainda usa a conexão direta (LAN/IPv6) e guarda
  apenas um computador pareado por vez. Para o caminho obrigatório via VPS no iPhone, ainda é necessário implementar o
  transporte relay em `PCLink.swift` e assinar uma nova build iOS. O VPS e os pacotes Windows já ficam prontos para isso.
- Android e iPhone atuais guardam um computador pareado por vez. Para mudar de computador, escaneie o QR do computador
  desejado; suporte a um seletor de vários computadores no mesmo celular é uma evolução separada da frota do VPS.

## Atualização sem perder a frota

Dentro do checkout completo:

```sh
git pull --ff-only
cd android/relay
./vpsctl up
./vpsctl doctor
```

Os arquivos em `state/` e `.env` são ignorados pelo Git e permanecem intactos. Antes de uma mudança maior, rode
`./vpsctl backup`.
