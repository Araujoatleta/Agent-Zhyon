# Runbook: deploy, backup, restore e migração do Hermes no VPS

Este fluxo move apenas o estado durável selecionado do Hermes e implanta o `vps-control` sem substituir o relay Coucou existente. Execute como operador com acesso ao Docker e ao Nginx. Nunca coloque tokens, `.env`, chaves ou arquivos de autenticação em commits, tickets ou saída de CI.

## Pré-requisitos e limites

- Python 3.11+, `tar`, Docker com Compose v2, Nginx, `curl` e `systemctl` no VPS.
- O relay existente deve continuar atendendo antes e depois do deploy. O script exige seu URL em `--relay-health` em produção.
- O arquivo de ambiente fica fora do checkout e deve ter modo `0600`. `ADMIN_TOKEN` deve ter pelo menos 24 caracteres.
- O backup inclui apenas configuração e integrações duráveis. Exclui caches, logs, sessões, sockets, estado volátil, toolchains, `node_modules`, bancos WAL/SHM e diretórios de build.
- Crons não devem rodar simultaneamente na origem e no destino. Desabilite-os na origem durante o cutover e habilite-os no destino somente depois da validação.

## Backup seletivo

```sh
umask 077
python3 scripts/vps_ops.py --dry-run backup "$HERMES_HOME" /secure/hermes-$(date -u +%Y%m%dT%H%M%SZ).tgz
python3 scripts/vps_ops.py backup "$HERMES_HOME" /secure/hermes-$(date -u +%Y%m%dT%H%M%SZ).tgz
python3 scripts/vps_ops.py validate /secure/hermes-YYYYMMDDTHHMMSSZ.tgz
```

O arquivo contém `manifest.json`, tamanho e SHA-256 de cada membro e recebe modo `0600`. Symlinks não são seguidos. A validação rejeita caminhos absolutos, `..`, links, devices e membros fora de `payload/`. Guarde o arquivo em armazenamento criptografado e apague cópias temporárias ao final.

## Transferência e restauração

Transfira por SSH/SCP para um diretório de staging privado. Não passe segredos na linha de comando:

```sh
scp /secure/hermes-....tgz operator@novo-vps:/secure-staging/
ssh operator@novo-vps 'chmod 600 /secure-staging/hermes-....tgz'
```

No destino, valide e simule primeiro:

```sh
python3 scripts/vps_ops.py validate /secure-staging/hermes-....tgz
python3 scripts/vps_ops.py --dry-run restore /secure-staging/hermes-....tgz "$HERMES_HOME" --backup-dir /secure/backups
python3 scripts/vps_ops.py restore /secure-staging/hermes-....tgz "$HERMES_HOME" --backup-dir /secure/backups
```

A restauração valida todos os checksums **antes** da primeira mutação e cria `pre-restore-*.tgz` do estado correspondente no destino. Ela mescla somente os arquivos do manifesto; não apaga configuração local fora desse conjunto. Para migração entre diretórios montados localmente, `migrate` combina backup e restore:

```sh
python3 scripts/vps_ops.py --dry-run migrate /mnt/origem-hermes /srv/hermes --backup-dir /secure/backups
python3 scripts/vps_ops.py migrate /mnt/origem-hermes /srv/hermes --backup-dir /secure/backups
```

Rollback de estado usa exatamente a mesma validação e também gera um novo backup pré-rollback:

```sh
python3 scripts/vps_ops.py rollback /secure/backups/pre-restore-TIMESTAMP.tgz "$HERMES_HOME" --backup-dir /secure/backups
```

## Configuração do Nginx sem substituir o relay

Dentro do `server` TLS existente, adicione uma única vez:

```nginx
include /etc/nginx/snippets/coucou-control.conf;
```

Não edite as rotas `/` ou `/v1/room` do relay. O deploy instala somente o snippet versionado. Backups de configuração Nginx não devem ficar em `sites-enabled`.

## Deploy com release atômica

Crie `/secure/coucou-control.env` (fora do repositório):

```sh
umask 077
printf 'ADMIN_TOKEN=' > /secure/coucou-control.env
openssl rand -hex 32 >> /secure/coucou-control.env
chmod 600 /secure/coucou-control.env
```

Não exiba esse arquivo. Simule e depois execute:

```sh
sudo vps-control/deploy/deploy.sh deploy \
  --source "$PWD" --target /opt/agent-zhyon-control \
  --env-file /secure/coucou-control.env \
  --nginx-server-config /etc/nginx/sites-enabled/coucou-relay.navellux.com.br \
  --relay-health https://coucou-relay.navellux.com.br/ \
  --control-health https://coucou-relay.navellux.com.br/control/healthz \
  --dry-run

sudo vps-control/deploy/deploy.sh deploy [mesmos argumentos sem --dry-run]
```

Cada deploy cria uma release imutável, preserva a anterior, usa o nome Compose fixo `coucou-control` (portanto mantém o volume existente), valida `docker compose config`, sobe o container, verifica health, valida/recarrega Nginx e verifica novamente o relay. O symlink `current` só muda após os checks.

Rollback do aplicativo:

```sh
sudo vps-control/deploy/deploy.sh rollback \
  --target /opt/agent-zhyon-control \
  --env-file /secure/coucou-control.env \
  --nginx-server-config /etc/nginx/sites-enabled/coucou-relay.navellux.com.br \
  --relay-health https://coucou-relay.navellux.com.br/ \
  --control-health https://coucou-relay.navellux.com.br/control/healthz
```

Também é possível indicar `--release /opt/agent-zhyon-control/releases/RELEASE`. Não remova a release anterior nem o backup do destino antes da janela de observação.

## Verificação de cutover

1. `hermes config check`; liste MCPs e teste cada servidor sem imprimir tokens.
2. Confirme local e publicamente `/control/healthz`.
3. Confirme que `/` e `/v1/room` continuam no relay existente.
4. Verifique `docker compose --project-name coucou-control ps` e logs recentes sem despejar o ambiente.
5. Execute uma chamada somente leitura para cada integração. “Credencial copiada” não significa “integração autenticada”.
6. Inicie um novo processo/sessão Hermes para recarregar ferramentas; só então habilite crons no destino.
7. Apague staging, arquivo transferido e autorização SSH temporária. Retenha o backup pré-migração em armazenamento criptografado.

Se qualquer health check falhar, o script não promove `current`. Execute `rollback`, restaure o backup de estado se necessário e confirme novamente tanto control plane quanto relay.
