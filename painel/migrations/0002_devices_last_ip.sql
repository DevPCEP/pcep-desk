-- IP de origem do último heartbeat: o histórico (audit/conn) só é aceito de
-- quem acabou de mandar heartbeat do mesmo IP.
ALTER TABLE devices ADD COLUMN last_ip TEXT NOT NULL DEFAULT '';
