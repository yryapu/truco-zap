# truco-zap

**Truco Paulista** jogável no navegador, 1x1 e 2x2, servidor em Rust, comunicação em tempo
real por WebSocket onde **a carta é o próprio caractere Unicode**:

```
🂡  🂱  🃁  🃑
```

Pesquisa, fontes das regras, decisões de desenho e erros:
**https://github.com/yryapu/poliorketikos-truco-zap**

## Subir e testar

```bash
docker compose up -d --build          # sobe tudo; abra http://localhost:18080
docker compose --profile teste run --rm e2e   # testa o front (Playwright, dentro do Docker)
cargo test                            # testa as regras e o servidor
```

A porta do host é configurável (`TRUCO_PORTA=9090 docker compose up -d`) porque 8080 colide
com frequência. Dentro da rede do Compose o serviço é sempre `app:8080`.

## O que tem

| | |
|---|---|
| **Cadastro rápido** | botão *Jogar agora*: um clique cria a conta, o cookie e já leva ao lobby. Sem formulário, sem e-mail. Conta com apelido/senha é opcional e vem depois. |
| **Sessão isolada** | token opaco de 32 bytes em cookie `HttpOnly; SameSite=Lax`, guardado no banco **só como SHA-256**. Senha com Argon2id. Nenhuma rota pessoal responde sem sessão válida. |
| **Saldo** | todo jogador começa com **1000 moedas**. Aposta escolhida por partida; debitada na abertura e paga no fim, dentro de transação. O saldo não tem valor fora do jogo. |
| **Ranking e emblemas** | ranking por vitórias, saldo e saldo de partidas. Emblemas em duas trilhas: por vitórias (Estreante → Pé-de-meia → Bom de truco → Zap → Lenda) e por histórico (Veterano, Invicto, Ressurgido, Fortuna, Quebrado). |
| **Webhooks** | registre uma URL e receba `partida.comecou`, `partida.terminou`, `partida.resultado` por POST, assinados com `X-Truco-Signature: sha256=<HMAC-SHA256 do corpo>`. Cada evento traz `evento_id` — a entrega é *at-least-once*, então seja idempotente. |
| **Tempo real** | um WebSocket por jogador, servidor autoritativo. O cliente desenha o que recebe e manda intenção; ele não conhece nenhuma regra. |

## As regras

Estão especificadas, **com a fonte de cada uma**, em
[REGRAS.md](https://github.com/yryapu/poliorketikos-truco-zap/blob/main/REGRAS.md) (R1–R16), e
testadas em [`truco_core/tests/regras.rs`](truco_core/tests/regras.rs) — incluindo a tabela
exaustiva de 15 resultados de empate transcrita de pagat.com e um fuzz de 2000 partidas inteiras.

Três coisas que surpreendem quem nunca jogou paulista, e que estão implementadas:

- **A Dama vale menos que o Valete.** Ordem: `4 < 5 < 6 < 7 < Q < J < K < A < 2 < 3`.
- **A manilha muda a cada mão.** É o rank imediatamente acima da *vira*, em ordem cíclica —
  vira 3 ⇒ manilha 4. E só entre manilhas o naipe desempata (♣ > ♥ > ♠ > ♦).
- **Ganhar uma rodada e empatar outra ganha a mão.** Se a primeira empata, ganha quem vencer
  a próxima. Três empates: ninguém pontua.

Duas escolhas deliberadas onde as fontes divergem, para não parecerem bug:

- depois de uma rodada empatada, puxa **quem jogou a primeira carta que empatou**
  ([D07](https://github.com/yryapu/poliorketikos-truco-zap/blob/main/decisoes/D07-lider-apos-empate.md));
- pedir truco na mão de onze é **comando recusado**, não derrota automática
  ([D10](https://github.com/yryapu/poliorketikos-truco-zap/blob/main/decisoes/D10-truco-na-mao-de-onze.md)).

## Arquitetura

```
truco_core/     motor de regras. Puro: sem I/O, sem async, sem rede. É o que mantém
                aberta a opção de WASM no cliente (hoje não usada, e por quê: D06).
truco_server/   axum + tokio. HTTP e WebSocket no mesmo processo e na mesma porta.
  db.rs         SQLite via sqlx. Saldo e webhooks na mesma transação do resultado.
  auth.rs       argon2 + token opaco. O extractor `Autenticado` é o único caminho
                para uma rota pessoal — não há porta dos fundos.
  hub.rs        mesas vivas em memória, protocolo WS, política do robô.
  webhooks.rs   guarda de SSRF + entrega em background com HMAC.
  emblemas.rs   função pura do perfil para a lista de emblemas.
static/         HTML, CSS e JS puros. Sem build step, sem node_modules.
e2e/            Playwright, rodando dentro do Docker.
```

**Bibliotecas, e por quê:** `axum` (upgrade de WebSocket como extractor, middleware do
ecossistema tower, mesmo processo para HTTP e WS) · `tokio` · `sqlx`+SQLite (ACID para o
saldo sem cobrar um segundo container) · `argon2` · `reqwest`+rustls · `hmac`/`sha2` ·
`serde`. As alternativas descartadas e o motor de cada escolha estão em
[`decisoes/`](https://github.com/yryapu/poliorketikos-truco-zap/tree/main/decisoes).

**Por que não WASM no cliente:** o servidor é autoritativo, o cliente não tem regra para
rodar nem cálculo pesado, e o único "tipo compartilhado" interessante — a carta — já é um
caractere. WASM cobraria um toolchain e centenas de KB antes da primeira carta, contra o
requisito de "jogando em menos de um minuto". Os quatro motivos e o critério que me faria
mudar de ideia: [D06](https://github.com/yryapu/poliorketikos-truco-zap/blob/main/decisoes/D06-wasm-nao.md).

## O protocolo WebSocket

Cliente → servidor:

```json
{"t":"entrar_fila","modo":"1x1","aposta":100}
{"t":"jogar","carta":"🃑","encoberta":false}
{"t":"pedir"}   {"t":"aceitar"}   {"t":"aumentar"}   {"t":"correr"}
{"t":"mao_de_onze","jogar":true}
```

Servidor → cliente: `ola`, `fila`, `mesa`, `estado`, `evento`, `erro`, `fim`.

`estado` é a visão **daquele** jogador: ele recebe a própria mão e nunca a dos outros. A
única exceção é a mão do parceiro durante a decisão da mão de onze, que é regra do jogo (R10).
Carta encoberta trafega como `🂠` (U+1F0A0, *playing card back*) — o caractere Unicode certo
para "de costas", não um símbolo inventado.

Qualquer carta fora das 40 do baralho sujo é recusada com `carta_desconhecida`, inclusive o
*Cavaleiro* (U+1F0AC, U+1F0BC, U+1F0CC, U+1F0DC), que existe no Unicode e não existe no truco.

## Robôs

Se ninguém mais estiver na fila por 8 segundos, a mesa é completada com robôs, para que o
requisito "começa a jogar em menos de um minuto" seja verdade com a casa vazia. Robô não
aposta: o bolo é só o que humanos colocaram, e é dividido entre os humanos vencedores — não
existe caminho para farmar moedas contra robô.

## Variáveis de ambiente

| variável | padrão | para que serve |
|---|---|---|
| `PORT` | `8080` | porta dentro do container |
| `DATABASE_URL` | `sqlite:///dados/truco.db` | banco |
| `TRUCO_STATIC` | `/srv/static` | diretório dos estáticos |
| `TRUCO_COOKIE_SECURE` | desligado | liga `Secure` no cookie — **ligue atrás de TLS** |
| `TRUCO_WEBHOOK_ALLOW_HTTP` | desligado | permite webhook em `http://` |
| `TRUCO_WEBHOOK_ALLOW_PRIVATE` | desligado | permite webhook para endereço privado |

As duas últimas estão ligadas no `docker-compose.yml` **só** porque o receptor de webhook dos
testes roda na rede do Compose. Em produção elas ficam fora: é o que mantém a guarda de SSRF
ligada (recusa loopback, privado, link-local, CGNAT e IPv4-mapeado, revalidando na entrega e
fixando o endereço validado na conexão para fechar DNS rebinding).

## Riscos conhecidos

Estão listados em [`resultado.json`](resultado.json), campo `riscos_conhecidos`, sem
maquiagem.
