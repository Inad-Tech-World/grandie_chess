// The coach's side of the screen: what it's saying now (also spoken), the
// playback controls for a demonstration, the conversation so far, a box to
// ask questions and the hold-to-talk microphone.

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ActionIcon, Anchor, Badge, Button, Group, Loader, Paper, Progress, ScrollArea, Stack, Text, TextInput, Tooltip } from "@mantine/core";
import {
  IconMicrophone,
  IconPlayerPause,
  IconPlayerPlay,
  IconPlayerSkipForward,
  IconPlayerStop,
  IconRefresh,
  IconSend,
} from "@tabler/icons-react";
import { commands, events, type AskContext, type ChatTurn, type CoachInfo, type VoiceCommand, type VoiceResult } from "../bindings";
import { errorKind, errorMessage } from "../lib/api";
import { useApp } from "../stores/app";
import { useCoach, useTranscript } from "../stores/coach";
import { useSettings } from "../stores/settings";
import { useVoice } from "../voice/useVoice";

export interface CoachPanelProps {
  /** Which screen owns the coach, e.g. "analysis:<tab>". */
  owner: string;
  /** The board as the coach should see it, for questions. */
  context: () => AskContext;
  /** The position spoken moves are played in (null: no voice moves). */
  fen: () => string | null;
  onVoiceMove?: (uci: string, san: string) => void;
  /** Return true when the screen handled the command. */
  onVoiceCommand?: (command: VoiceCommand) => boolean | void;
  /** Screen-specific coach buttons ("Coach me through this game", ...). */
  actions?: ReactNode;
  placeholder?: string;
}

let coachList: Promise<CoachInfo[]> | null = null;

function useCoachName(): string {
  const coachId = useSettings((s) => s.settings.coach.coachId);
  const [list, setList] = useState<CoachInfo[]>([]);
  useEffect(() => {
    coachList ??= commands.coachList().catch(() => []);
    let live = true;
    void coachList.then((l) => live && setList(l));
    return () => {
      live = false;
    };
  }, []);
  const coach = list.find((c) => c.id === coachId && c.available) ?? list.find((c) => c.default) ?? list[0];
  return coach?.name ?? "Coach";
}

export function CoachPanel({ owner, context, fen, onVoiceMove, onVoiceCommand, actions, placeholder }: CoachPanelProps) {
  const mine = useCoach((s) => s.owner === owner);
  const status = useCoach((s) => (s.owner === owner ? s.status : "idle"));
  const caption = useCoach((s) => (s.owner === owner ? s.caption : null));
  const loadingLabel = useCoach((s) => (s.owner === owner ? s.loadingLabel : null));
  const notice = useCoach((s) => (s.owner === owner ? s.notice : null));
  const script = useCoach((s) => (s.owner === owner ? s.script : null));
  const transcript = useTranscript(owner);
  const coach = useCoach.getState;
  const aiEnabled = useSettings((s) => s.settings.ai.enabled);
  const setSection = useApp((s) => s.setSection);
  const name = useCoachName();
  const [question, setQuestion] = useState("");
  const [asking, setAsking] = useState<string | null>(null);
  const [tool, setTool] = useState<string | null>(null);
  const [error, setError] = useState<{ message: string; needsKey: boolean } | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Stop the coach when this screen goes away.
  useEffect(() => () => useCoach.getState().release(owner), [owner]);

  const lastText = transcript[transcript.length - 1]?.text;
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [transcript.length, lastText]);

  const history = (): ChatTurn[] =>
    transcript
      .filter((e) => !e.pending && e.text.trim())
      .slice(-12)
      .map((e) => ({ from: e.from, text: e.text }));

  async function ask(text: string) {
    const q = text.trim();
    if (!q || asking) return;
    setError(null);
    setQuestion("");
    const turns = history();
    coach().say(owner, q, "user");
    const entry = coach().say(owner, "", "coach");
    coach().updateEntry(entry, { pending: true });
    const requestId = `ask-${Date.now()}`;
    setAsking(requestId);
    let answer = "";
    const unlisten = await events.coachEvent.listen((e) => {
      if (e.payload.requestId !== requestId) return;
      const k = e.payload.kind;
      if (k.type === "delta") {
        answer += k.text;
        coach().updateEntry(entry, { text: answer });
      } else if (k.type === "tool") {
        setTool(k.name);
      } else if (k.type === "script") {
        coach().enqueue(k.script, owner);
      }
    });
    try {
      const r = await commands.coachAsk(requestId, context(), q, turns, null);
      if (r.status === "error") throw r.error;
      coach().updateEntry(entry, { text: r.data, pending: false });
      coach().speak(owner, r.data);
    } catch (e) {
      const kind = errorKind(e);
      coach().updateEntry(entry, { text: kind === "cancelled" ? "(stopped)" : "", pending: false });
      if (kind !== "cancelled") setError({ message: errorMessage(e), needsKey: kind === "noApiKey" || kind === "aiQuota" });
    } finally {
      unlisten();
      setAsking(null);
      setTool(null);
    }
  }

  function stopAll() {
    if (asking) void commands.coachCancel(asking);
    coach().stop();
  }

  function onVoice(r: VoiceResult) {
    const intent = r.intent;
    switch (intent.type) {
      case "move":
        if (onVoiceMove) onVoiceMove(intent.uci, intent.san);
        else void ask(r.transcript);
        break;
      case "ambiguous":
        coach().say(owner, r.transcript, "user");
        coach().say(owner, `Which one: ${intent.options.join(" or ")}?`);
        coach().speak(owner, `Which one: ${intent.options.join(" or ")}?`);
        break;
      case "noMatch":
        coach().say(owner, r.transcript, "user");
        coach().say(owner, `I heard "${intent.heard}", but that isn't a legal move here.`);
        coach().speak(owner, "That isn't a legal move here.");
        break;
      case "command": {
        if (onVoiceCommand?.(intent.command)) break;
        if (intent.command === "stop") stopAll();
        else if (intent.command === "repeat") coach().replay();
        else if (intent.command === "explain") void ask("Why? Explain the idea in this position.");
        else if (intent.command === "bestMove") void ask("What is the best move here, and why?");
        break;
      }
      case "question":
        void ask(intent.text);
        break;
      case "empty":
        break;
    }
  }

  const voice = useVoice({ fen, onResult: onVoice, enabled: aiEnabled });
  const playing = status === "playing" || status === "paused";
  const lastScriptId = useMemo(() => [...transcript].reverse().find((e) => e.scriptId)?.scriptId, [transcript]);

  return (
    <Paper withBorder radius="md" p="sm" className="coach-panel" aria-label="Coach">
      <Group justify="space-between" mb={6} wrap="nowrap">
        <Group gap={8} wrap="nowrap">
          <div className="coach-avatar" aria-hidden>
            {name.slice(0, 1)}
          </div>
          <div>
            <Text fw={650} size="sm" lh={1.2}>
              {name}
            </Text>
            <Text size="xs" c="dimmed" lh={1.2}>
              {status === "loading" ? loadingLabel : asking ? (tool ? "Checking with the engine…" : "Thinking…") : playing ? "Showing you on the board" : "Your coach"}
            </Text>
          </div>
        </Group>
        <Group gap={4} wrap="nowrap">
          {script?.aiNarrated && (
            <Tooltip label="Words written by the AI model; the moves come from the engine">
              <Badge size="xs" variant="light">
                AI
              </Badge>
            </Tooltip>
          )}
          {playing && (
            <>
              {status === "paused" ? (
                <ActionIcon variant="subtle" aria-label="Resume" onClick={() => coach().resume()}>
                  <IconPlayerPlay size={16} />
                </ActionIcon>
              ) : (
                <ActionIcon variant="subtle" aria-label="Pause" onClick={() => coach().pause()}>
                  <IconPlayerPause size={16} />
                </ActionIcon>
              )}
              <ActionIcon variant="subtle" aria-label="Skip" onClick={() => coach().skip()}>
                <IconPlayerSkipForward size={16} />
              </ActionIcon>
            </>
          )}
          {mine && lastScriptId && !playing && (
            <ActionIcon variant="subtle" aria-label="Replay" onClick={() => coach().replay(lastScriptId)}>
              <IconRefresh size={16} />
            </ActionIcon>
          )}
          {(playing || asking) && (
            <ActionIcon variant="subtle" color="red" aria-label="Stop the coach" onClick={stopAll}>
              <IconPlayerStop size={16} />
            </ActionIcon>
          )}
        </Group>
      </Group>

      {(status === "loading" || caption) && (
        <div className="coach-caption" aria-live="polite">
          {status === "loading" ? (
            <Group gap={8}>
              <Loader size="xs" />
              <Text size="sm">{loadingLabel}</Text>
            </Group>
          ) : (
            <Text size="sm">{caption}</Text>
          )}
        </div>
      )}
      {notice && (
        <Text size="xs" c="yellow.8" mt={4}>
          {notice}
        </Text>
      )}

      {actions && (
        <Group gap={6} mt={8}>
          {actions}
        </Group>
      )}

      {transcript.length > 0 && (
        <ScrollArea.Autosize mah={220} mt={8} viewportRef={scrollRef}>
          <Stack gap={6} aria-label="Conversation with the coach">
            {transcript.map((e) => (
              <div key={e.id} className={e.from === "user" ? "coach-bubble user" : "coach-bubble"}>
                {e.pending && !e.text ? <Loader size="xs" type="dots" /> : <Text size="sm">{e.text}</Text>}
                {e.scriptId && !playing && (
                  <Anchor size="xs" component="button" onClick={() => coach().replay(e.scriptId)}>
                    Show again
                  </Anchor>
                )}
              </div>
            ))}
          </Stack>
        </ScrollArea.Autosize>
      )}

      {error && (
        <Text size="xs" c="red" mt={6} role="alert">
          {error.message}{" "}
          {error.needsKey && (
            <Anchor size="xs" component="button" onClick={() => setSection("settings")}>
              Open settings
            </Anchor>
          )}
        </Text>
      )}
      {voice.error && (
        <Text size="xs" c="red" mt={6} role="alert">
          {voice.error}
        </Text>
      )}

      <Group gap={6} mt={8} wrap="nowrap">
        <TextInput
          style={{ flex: 1 }}
          size="xs"
          placeholder={aiEnabled ? placeholder ?? "Ask the coach… (or hold V to talk)" : "Add an OpenRouter key in Settings to ask questions"}
          value={question}
          disabled={!aiEnabled || !!asking}
          onChange={(e) => setQuestion(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void ask(question);
          }}
          aria-label="Ask the coach"
        />
        <ActionIcon variant="light" aria-label="Send question" disabled={!aiEnabled || !question.trim() || !!asking} onClick={() => void ask(question)}>
          <IconSend size={16} />
        </ActionIcon>
        <Tooltip label={aiEnabled ? "Hold to talk (or hold V)" : "Voice needs an OpenRouter key"}>
          <ActionIcon
            variant={voice.listening ? "filled" : "light"}
            color={voice.listening ? "red" : undefined}
            aria-label="Hold to talk"
            disabled={!aiEnabled}
            onMouseDown={() => void voice.start()}
            onMouseUp={() => void voice.stop()}
            onMouseLeave={() => voice.listening && void voice.stop()}
          >
            {voice.busy ? <Loader size={14} /> : <IconMicrophone size={16} />}
          </ActionIcon>
        </Tooltip>
      </Group>
      {voice.listening && <Progress value={voice.level * 100} size={3} mt={4} aria-label="Microphone level" />}
      {!aiEnabled && (
        <Button variant="subtle" size="compact-xs" mt={4} onClick={() => setSection("settings")}>
          Set up the AI coach
        </Button>
      )}
    </Paper>
  );
}
