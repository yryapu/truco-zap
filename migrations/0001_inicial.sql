-- Esquema inicial. SQLite (decisão D02): o estado durável aqui é pequeno e de baixa
-- contenção, e saldo exige transação — SQLite dá ACID sem cobrar um serviço a mais.

CREATE TABLE jogador (
    id          TEXT    PRIMARY KEY,
    apelido     TEXT    NOT NULL,
    apelido_lc  TEXT    NOT NULL UNIQUE, -- unicidade case-insensitive sem depender de COLLATE
    senha_hash  TEXT    NOT NULL,
    saldo       INTEGER NOT NULL DEFAULT 1000,
    vitorias    INTEGER NOT NULL DEFAULT 0,
    derrotas    INTEGER NOT NULL DEFAULT 0,
    convidado   INTEGER NOT NULL DEFAULT 0,
    criado_em   TEXT    NOT NULL,
    CHECK (saldo >= 0)
);

-- Sessão: guardamos só o SHA-256 do token (D03). Vazar esta tabela não entrega sessões.
CREATE TABLE sessao (
    token_hash  TEXT    PRIMARY KEY,
    jogador_id  TEXT    NOT NULL REFERENCES jogador(id) ON DELETE CASCADE,
    criada_em   TEXT    NOT NULL
);
CREATE INDEX idx_sessao_jogador ON sessao(jogador_id);

CREATE TABLE partida (
    id            TEXT    PRIMARY KEY,
    modo          TEXT    NOT NULL,   -- '1x1' | '2x2'
    aposta        INTEGER NOT NULL,
    estado        TEXT    NOT NULL,   -- 'em_andamento' | 'terminada' | 'cancelada'
    placar_time0  INTEGER,
    placar_time1  INTEGER,
    time_vencedor INTEGER,
    bolo          INTEGER NOT NULL DEFAULT 0,
    comecou_em    TEXT    NOT NULL,
    terminou_em   TEXT
);
CREATE INDEX idx_partida_estado ON partida(estado);

CREATE TABLE participacao (
    partida_id  TEXT    NOT NULL REFERENCES partida(id) ON DELETE CASCADE,
    jogador_id  TEXT    NOT NULL REFERENCES jogador(id) ON DELETE CASCADE,
    assento     INTEGER NOT NULL,
    time        INTEGER NOT NULL,
    aposta      INTEGER NOT NULL,
    premio      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (partida_id, jogador_id)
);
CREATE INDEX idx_participacao_jogador ON participacao(jogador_id);

CREATE TABLE webhook (
    id          TEXT    PRIMARY KEY,
    jogador_id  TEXT    NOT NULL REFERENCES jogador(id) ON DELETE CASCADE,
    url         TEXT    NOT NULL,
    segredo     TEXT    NOT NULL,
    ativo       INTEGER NOT NULL DEFAULT 1,
    criado_em   TEXT    NOT NULL
);
CREATE INDEX idx_webhook_jogador ON webhook(jogador_id);

-- Fila de entrega (D12). Enfileirada na MESMA transação que grava o resultado:
-- se o resultado existe, o evento vai ser entregue (at-least-once).
CREATE TABLE entrega (
    id            TEXT    PRIMARY KEY,
    webhook_id    TEXT    NOT NULL REFERENCES webhook(id) ON DELETE CASCADE,
    evento        TEXT    NOT NULL,
    corpo         TEXT    NOT NULL,
    status        TEXT    NOT NULL DEFAULT 'pendente', -- pendente|entregue|falhou
    tentativas    INTEGER NOT NULL DEFAULT 0,
    ultimo_erro   TEXT,
    criada_em     TEXT    NOT NULL,
    atualizada_em TEXT
);
CREATE INDEX idx_entrega_status ON entrega(status);
