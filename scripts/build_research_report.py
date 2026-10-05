"""Assemble the research findings and illustrated report from completed Rust runs.

Invoke build(Path('output/research')) from a Python session. Dependencies:
ijson==3.5.1, zstandard==0.25.0, markdown==3.11. No separate CLI parser.
Statistical estimates and their confidence intervals come from the Rust
analyses; this file formats those reports and descriptive coverage summaries.
"""
from pathlib import Path
import csv
import datetime as dt
import json
import re
import shutil
import statistics
import subprocess
import ijson
import markdown
import zstandard

RANKS = ['Coal', 'Iron', 'Gold', 'Emerald', 'Diamond', 'Netherite']


def build(directory=Path('output/research'), seasons=(10, 11)):
    seasons = tuple(seasons)
    main = seasons == (10, 11)
    directory = Path(directory)
    cache = max(directory.glob('forfeit_research_*.json.zst'), key=lambda p: p.stat().st_mtime)

    def items(prefix):
        with cache.open('rb') as f, zstandard.ZstdDecompressor().stream_reader(f) as stream:
            yield from ijson.items(stream, prefix + '.item', use_float=True)

    audits = []
    for a in items('audits'):
        audits.append(a)
    if tuple(a['season'] for a in audits) != seasons:
        raise ValueError(f'Report cache must contain exactly seasons {seasons}')
    pooled = []
    for c in items('recovery'):
        if c['key']['season'] == 0 and c['key']['rank'] == 7 and c['key']['view'] == 0:
            pooled.append(c)
    profile_groups = {r: [] for r in range(6)}
    for p in items('profiles'):
        b = p['total']
        if b['games'] >= 100 and p['rank'] < 6:
            profile_groups[p['rank']].append((b['quits']/b['games'], b['wins']/b['games'], b['pace']['n']/b['games']))
    print('Read source measurements', flush=True)

    def rows(name):
        with (directory/(name+'.csv')).open() as f:
            return list(csv.DictReader(f))

    def text(name):
        return (directory/(name+'.txt')).read_text()

    def count(name, label):
        match = re.search(re.escape(label) + r': (\d+)', text(name))
        if not match:
            raise ValueError(f'Missing cohort count in {name}: {label}')
        return int(match.group(1))

    def slope(name, factor=1):
        match = re.search(r'Change for a 10-point increase in the earlier rate: ([\d.\-]+) \[95% ([\d.\-]+),([\d.\-]+)\]', text(name))
        if not match:
            return 'unavailable'
        b, lo, hi = [float(x)*factor for x in match.groups()]
        return f'{b:+.2f} (95% CI {lo:+.2f} to {hi:+.2f})'

    def estimate(name):
        match = re.search(r'Change for a 10-point increase in the earlier rate: ([\d.\-]+) \[95% ([\d.\-]+),([\d.\-]+)\]', text(name))
        if not match:
            raise ValueError(f'Missing estimate in {name}')
        return tuple(float(x) for x in match.groups())

    def relation(name, rate, outcome):
        _, lo, hi = estimate(name)
        if lo > 0:
            return f'A higher earlier {rate} is associated with a higher later {outcome}.'
        if hi < 0:
            return f'A higher earlier {rate} is associated with a lower later {outcome}.'
        return f'The {rate} comparison gives no clear direction for later {outcome}. Its confidence interval includes zero.'

    def table(header, data):
        return '\n'.join(['| '+' | '.join(header)+' |', '| '+' | '.join('---' for _ in header)+' |']+['| '+' | '.join(map(str,row))+' |' for row in data])

    def prob(row):
        return f"{float(row['mean'])*100:.1f}% ({float(row['low_95'])*100:.1f}–{float(row['high_95'])*100:.1f})"

    def by_series(name):
        return {(r['series'], int(float(r['x']))): r for r in rows(name)}

    death = by_series('recovery_winrate')
    n = sum(a['played'] for a in audits)
    missing_clock = sum(a['missing_duration'] for a in audits)
    scope = 'seasons 10 and 11' if main else 'seasons 1 to 12'
    summary = [
        '# Win chances after death',
        f'Research report · 5 October 2026 · {scope.capitalize()}',
        '**Players can still win after an ordinary death. The win rate exceeds 5% in each of the six ranks.**',
        f'This report uses **{n:,} real ranked matches from {scope}**. It excludes decayed matches and other match types. '
        'It uses individual season files. It excludes their combined copy to prevent duplicate matches.',
        'Each figure appears next to its explanation. Select a figure to open the 2800×1600 image. Select its CSV link to inspect the values.',
        'The [source claim](https://x.com/FlanMCSR/status/2106585696047894564?s=20) in `note.md` says that players have almost no win chance after death. '
        'We defined this claim as a win chance below **5%** before the analysis. This report examines ordinary deaths. It ignores world resets.',
        '## Terms used in this report',
        table(['Term','Meaning'],[
            ['Ordinary death','A recorded death other than an intentional spawnpoint death.'],
            ['Continue play','You have an ordinary death and no recorded forfeit in that match. A later split is not necessary.'],
            ['Forfeit','A player has an explicit forfeit event in the timeline.'],
            ['Split','A recorded run milestone, such as nether entry or bastion entry.'],
            ['Rank','A fixed Elo range. Death figures use Elo before the match. Player profiles use mean Elo.'],
            ['Win rate','Wins divided by eligible matches. Draws remain in the match count.'],
            ['Pace gain','The proportional decrease in completion time. A positive value means faster completed runs.'],
            ['Group mean','The average outcome for players with similar earlier measurements.'],
            ['95% confidence interval','An estimate range that describes sampling uncertainty. It does not remove selection bias.'],
            ['Player record','One player in one season, or one player at a specified measurement day.'],
        ]),
        'A missing forfeit record does not prove that a player was active. The data can contain exits without an explicit forfeit event. '
        'Continue play is the definition that the available records permit.',
        table(['Rank','Elo range'], [['Coal','Below 600'],['Iron','600 to 899'],['Gold','900 to 1199'],['Emerald','1200 to 1499'],['Diamond','1500 to 1999'],['Netherite','2000 or more']]),
        '## 1. Win chances after death',
        'Use the first ordinary death in each player match. Include that match if the player does not forfeit. '
        'Exclude intentional spawnpoint deaths. A later split is not necessary. Ignore world resets throughout the analysis.',
        'Count wins with and without a completion. Count a draw as a match without a win. '
        'Stop observation at the first completion, the first forfeit by either player, or the recorded match end.',
        table(['Rank','Win rate after death and continued play'],
              [[rank,prob(death[('Continue play after death',i)])] for i,rank in enumerate(RANKS)]),
        f'The observed win rates range from {min(float(r["mean"])*100 for r in death.values()):.1f}% to {max(float(r["mean"])*100 for r in death.values()):.1f}%. '
        'The lower confidence limit exceeds 5% in all six ranks. Thus, the observed results do not support the claim for ordinary deaths. '
        'They do not prove that continued play is the best action after every death.',
        '[Win rate after death](recovery_winrate.png) · [How players win](recovery_outcomes.png)',
        '### The situation at time of death',
        'The split measurement uses the highest split recorded before death. World resets do not change this measurement. '
        'Thus, the split describes cumulative match history. It can differ from the position in the current attempt.',
        'A split lead does not measure inventory loss or the exact time difference. Each figure in this section uses matches without your forfeit.',
        'Gray cells have fewer than 100 matches or 20 players. The CSV retains their estimates and counts.',
    ]
    context = []
    for rank in ['Diamond','Netherite']:
        r = next(r for r in rows('death_rank_deficit') if r['rank']==rank and float(r['opponent_splits_ahead'])==2)
        context.append([rank,prob(r)])
    summary += [table(['Rank: opponent leads by at least two splits','Win rate'],context),
        'The table gives the estimate and its confidence interval for each group. Compare the interval with the 5% limit.',
        '[Rank and split lead](death_rank_deficit.png) · [Opponent split lead](recovery_deficit.png) · [Split at death](recovery_stage.png) · [Time of death](recovery_time.png) · [Earlier opponent death](recovery_opponent.png)',
        '### Opponent events after your death',
        'This section uses your first ordinary death in matches without your forfeit. It uses the same match count for both opponent event rates. '
        'A player who loses before the next split remains eligible.',
        'An opponent event must occur strictly after your death and before observation ends. Events at the same time do not count as later events. '
        'An opponent can die before your death and die again after your death. Exclude intentional spawnpoint deaths from both players.',
    ]
    opponent_frequency = by_series('opponent_after_death')
    opponent_wins = {(r['rank'], r['group'], int(r['behavior'])): r for r in rows('opponent_after_death_winrate')}
    post_forfeit_deaths = sum(a['post_forfeit_deaths'] for a in audits)
    summary += [
        f'The timing check found {post_forfeit_deaths:,} ordinary deaths recorded after a forfeit. The analysis excludes these deaths. '
        'A forfeit ends the match. An opponent death after your forfeit cannot help you win.',
        table(['Your rank','Opponent dies after you','Opponent forfeits after you','Your win rate: later opponent death','Your win rate: later opponent forfeit'],
            [[rank,prob(opponent_frequency[('Opponent dies after you',i)]),prob(opponent_frequency[('Opponent forfeits after you',i)]),prob(opponent_wins[(rank,'Any later death',1)]),prob(opponent_wins[(rank,'Any later forfeit',1)])] for i,rank in enumerate(RANKS)]),
        f'The later opponent death rate ranges from {min(float(opponent_frequency[("Opponent dies after you",i)]["mean"]) for i in range(6))*100:.1f}% '
        f'to {max(float(opponent_frequency[("Opponent dies after you",i)]["mean"]) for i in range(6))*100:.1f}% across ranks.',
        f'The later opponent forfeit rate ranges from {min(float(opponent_frequency[("Opponent forfeits after you",i)]["mean"]) for i in range(6))*100:.1f}% '
        f'to {max(float(opponent_frequency[("Opponent forfeits after you",i)]["mean"]) for i in range(6))*100:.1f}%.',
        'The opponent can die and then forfeit. Thus, the two event rates can overlap. These rates describe events per match. '
        'They do not describe events per minute. A longer match gives the opponent more time to die or forfeit.',
        '[Opponent events](opponent_after_death.png)',
        'The next comparison puts each match in one of four groups. The death-only group excludes later opponent forfeits. '
        'This separates wins after an opponent death from wins after an opponent forfeit.',
        table(['Your rank','No recorded death or forfeit','Death only','Forfeit only','Death and forfeit'],
            [[rank]+[prob(opponent_wins[(rank,group,1)]) for group in ['Neither','Death only','Forfeit only','Both']] for rank in RANKS]),
        '[Win rate by opponent result](opponent_after_death_winrate.png)',
        'The groups describe events that occur after your death. You cannot know the future group at time of death. '
        'The differences show associations. They do not measure the increase in win chance caused by continued play.',
        'The no-event group means no recorded death or forfeit. An exit without a classified event can still produce a win. '
        'The CSV also gives all death matches as a separate reference. The figure uses only matches without your forfeit.',
        'The next figure separates opponent deaths before your death, after your death, at both times, and at neither time. '
        'Each group can also contain opponent forfeits.',
        '[Opponent death time](opponent_death_timing.png)',
        '### Results by season',
        f'This comparison uses {scope}. It uses the same death and continued play definitions in each season. '
        'The main pooled estimates combine the selected seasons.',
        '[Win rate by season](recovery_seasons.png)',
    ]
    season_rows = by_series('recovery_seasons')
    summary.append(table(['Season','Win rate after death and continued play'],[[f'S{s}',prob(season_rows[('Continue play after death',s)])] for s in seasons if ('Continue play after death',s) in season_rows]))
    summary += [
        '## 2. Elo gain per hour after death',
        'Start the comparison at the first ordinary death. Compare players who forfeit after death with players who continue play. '
        'A later split is not necessary. Ignore world resets.',
        'Both actions use the same weights for shared comparison groups. The groups share the season, Elo, opponent Elo difference, split, and split lead. '
        'Each action needs at least 20 observations in a group.',
        'The 10-game period includes the current match and nine later rated games. Time includes the remainder of this match and the later matches. '
        'Both actions need complete data for all 10 games. The same-session comparison permits gaps of at most 30 minutes.',
    ]
    eff = by_series('recovery_efficiency')
    higher = sum(float(eff[('Continue: 10 games',i)]['mean']) > float(eff[('Forfeit: 10 games',i)]['mean']) for i in range(6))
    summary += [
        f'Continued play has a higher observed Elo gain per game hour in **{higher} of six ranks** over the 10-game period.',
        table(['Rank','Forfeit: Elo per game hour','Continue play: Elo per game hour'],[[rank,f"{float(eff[('Forfeit: 10 games',i)]['mean']):+.2f}",f"{float(eff[('Continue: 10 games',i)]['mean']):+.2f}"] for i,rank in enumerate(RANKS)]),
        'A negative value means an Elo loss. The 10-game comparison includes later results after the player enters another match. '
        'The groups contain players who chose different actions. These results do not show cause and effect.',
        '**Actual elapsed time is unknown.** The export does not separate queue time, loading, breaks, penalties, or practice outside ranked matches. '
        'Thus, the analysis measures Elo per recorded game hour. It does not measure Elo per hour of your day.',
        '[Elo in this match](recovery_elo.png) · [Elo per game hour](recovery_efficiency.png)',
        '## 3. Forfeit rate and general performance',
        'Each player record needs at least 100 rated games in one season. Mean historical Elo determines the rank for these figures. '
        'Each player record has the same weight. The study uses all eligible records instead of the final top 200 players.',
        'The forfeit rate counts that player’s explicit forfeit events. These analyses retain forfeited matches to measure the rate. '
        'World resets have no effect on the records.',
        table(['Rank','Player records','Median forfeit rate','Mean win rate','Mean completion rate'],[[RANKS[r],len(v),f'{statistics.median(x[0] for x in v)*100:.1f}%',f'{statistics.mean(x[1] for x in v)*100:.1f}%',f'{statistics.mean(x[2] for x in v)*100:.1f}%'] for r,v in profile_groups.items() if v]),
        'Compare completion time with completion rate. A player can forfeit slow attempts and then have a faster average for the completed runs. '
        'That pattern does not prove a skill improvement.',
        '[Elo](forfeit_rating.png) · [Win rate](forfeit_winrate.png) · [Completion time](forfeit_pace.png) · [Completion rate](forfeit_completion.png) · [Score difference](forfeit_residual.png) · [Elo prediction check](forfeit_calibration.png)',
        'The prediction check uses the conventional 400-point Elo formula. This formula is a reference, not a verified implementation of the game. '
        'Compare its predicted score with the actual score. A score difference does not measure skill alone.',
        '## 4. Forfeit rate and later improvement',
        'Measure earlier play in the last 50 rated games before season day 30, 60, or 90. Measure later outcomes in the next 30 days. '
        'Compare records with similar earlier measurements.',
        'Comparison groups share the season, measurement day, rank, earlier Elo, completion time, and earlier Elo gain. '
        'Each plotted outcome subtracts its group mean. The vertical axis uses “− mean” to show this subtraction.',
        'Use only groups in which the earlier rate differs between players. The confidence intervals account for repeated records from the same player. '
        'These comparisons require enough later activity. They do not show a learning effect caused by forfeiting or continued play.',
        table(['Increase the earlier rate by 10 percentage points','Associated later change (95% confidence interval)'],[
            ['Forfeit rate: Elo gain per 100 games',slope('forfeit-improvement')],
            ['Forfeit rate: Elo gain per game hour',slope('forfeit-hour-improvement')],
            ['Continued play rate: Elo gain per 100 games',slope('recovery-improvement')],
            ['Forfeit rate: pace gain in percentage points',slope('forfeit-pace-improvement',100)],
            ['Continued play rate: pace gain in percentage points',slope('recovery-pace-improvement',100)],
            ['Forfeit rate: death rate decrease in percentage points',slope('forfeit-death-improvement',100)],
        ]),
        relation('forfeit-improvement', 'forfeit rate', 'Elo gain'),
        relation('recovery-improvement', 'continued play rate', 'Elo gain'),
        relation('forfeit-pace-improvement', 'forfeit rate', 'pace gain'),
        relation('recovery-pace-improvement', 'continued play rate', 'pace gain'),
        'The continued play rate counts ordinary death matches without your forfeit. Death-free matches do not enter its denominator. '
        'A later split is not necessary. World resets have no effect.',
        'A positive pace gain means shorter completion times. Within-season pace comparisons match shared seed types and bastion layouts. '
        'Both periods need enough completed runs. An interval that includes zero does not give clear evidence of a direction.',
        f'The Elo comparison uses {count("forfeit-improvement", "Comparable player records"):,} player records. '
        f'The forfeit and pace comparison uses {count("forfeit-pace-improvement", "Comparable player records"):,} records.',
        f'Of {count("forfeit-improvement", "Records with enough earlier games"):,} records with enough earlier games, '
        f'{count("forfeit-improvement", "no later rated games"):,} have no later rated games. These counts can include the same player more than once.',
        'Players who forfeit often can continue to forfeit in the later period. This can directly affect later Elo. '
        'Motivation, inventory loss, and practice outside ranked matches are unknown. These factors can affect both the action and the result.',
        'A lower recorded death rate can also result from a forfeit before another death. It does not prove better execution.',
        '[Later Elo gain](forfeit_improvement.png) · [Later hourly Elo gain](forfeit_hour_improvement.png) · [Continue play and Elo](recovery_improvement.png) · [Later pace gain](forfeit_pace_improvement.png) · [Continue play and pace](recovery_pace_improvement.png) · [Later death rate](forfeit_death_improvement.png)',
        '### Improvement between seasons',
        ('Compare season 10’s last 50 games with season 11’s first 50 games.' if main else 'Compare the last 50 games with the first 50 games of each next adjacent season.'),
        'Use only players who return in the next season. Match completion times by seed type. '
        'The short periods do not give enough completed runs to match exact bastion layouts for most players.',
        'Compare Elo percentiles within each early or late game period. A percentile shows the player’s position in that period’s rating distribution. '
        'Player population changes and placement games can change this position.',
        table(['Increase the previous forfeit rate by 10 percentage points','Associated change in the next season (95% confidence interval)'],[
            ['Pace gain in percentage points',slope('forfeit-cross-season-pace',100)],
            ['Elo percentile change in percentage points',slope('forfeit-cross-season-rating',100)],
        ]),
        ('These estimates describe the season 10 to 11 transition only.' if main else 'These estimates combine the available adjacent season transitions.'),
        relation('forfeit-cross-season-pace', 'forfeit rate', 'pace gain between seasons'),
        relation('forfeit-cross-season-rating', 'forfeit rate', 'Elo percentile change between seasons'),
        'Returning players form a selected group. The pace comparison also requires completed runs. '
        'These results do not show cause and effect. Compare the return rate to assess this selection.',
        '[Pace gain between seasons](forfeit_cross_season_pace.png) · [Elo percentile change](forfeit_cross_season_rating.png) · [Return rate](forfeit_retention.png)',
        '## Data coverage and limits',
        f'The analysis excludes **{sum(a["excluded_death_resets"] for a in audits):,} intentional spawnpoint deaths**. '
        'These deaths contribute no death observations. Their matches retain valid results for the performance comparisons.',
        '[Excluded intentional deaths](death_reset_exclusions.png) · [Recorded match results](forfeit_coverage.png)',
        table(['Season','Real ranked matches','No completion flag','Missing timeline','Missing duration','Last match date (UTC)'],[[a['season'],f"{a['played']:,}",f"{a['flagged']/a['played']*100:.1f}%",f"{a['missing_timeline']:,}",f"{a['missing_duration']:,}",dt.datetime.fromtimestamp(a['end'],dt.timezone.utc).strftime('%Y-%m-%d')] for a in audits]),
        f'The results retain {missing_clock:,} matches without a measured duration. The last event can give a limit for event order. '
        'It does not give measured play time. Hourly Elo comparisons use measured durations only.',
        'The research loader checks duplicate match IDs. It retains parsed real ranked matches. Missing timeline events remain unknown.',
        'The [API documentation](https://docs.mcsrranked.com/) defines the match flag `forfeited` as no player completions. '
        'This flag differs from an explicit forfeit event for one player. A player can win without a completion or a recorded opponent forfeit.',
        ('Season 11 ends the main observation period. This report does not measure return into season 12.' if main else 'Season 12 is partial. Its observed matches end on 24 September 2026. Return into season 12 is therefore incomplete.'),
        'These are observed associations, not randomized action comparisons. The 5% limit was set before the analysis. '
        'Other group comparisons are exploratory. The analysis does not correct for multiple comparisons.',
    ]
    if main:
        summary += ['## Separate all-season comparison',
            'This figure uses all 12 available seasons. It does not contribute to the main season 10 and 11 estimates. '
            'Season 1 has no recorded ordinary deaths. Season 12 is partial and ends on 24 September 2026.',
            '[All-season win rate after death](all_seasons/recovery_seasons.png)',
            '[Complete all-season report](all_seasons/index.html). Its results remain separate from the main study.']
    else:
        summary += ['### Partial season 12',
            'These figures use season 12 only. Its partial coverage limits comparisons with completed seasons.',
            '[Season 12 win rate](season12_sensitivity/recovery_winrate.png) · [Season 12 forfeit rate and Elo](season12_sensitivity/forfeit_rating.png) · [Season 12 forfeit rate and win rate](season12_sensitivity/forfeit_winrate.png)']
    summary += ['## Generate and inspect the report',
        'Run this command from the repository directory.',
        '```sh\nbash scripts/run-forfeit-research.sh\n```',
        'Each figure has a separate Rust analysis. Its CSV gives the estimates and observation counts. Its text report gives the selection rules. '
        'The standard CLI input and season 11 default remain unchanged. The research script selects seasons 10 and 11 explicitly.',
        'The report uses ASD-STE100 writing rules and defined technical terms. '
        'The [official standard](https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf) contains the writing rules and dictionary.',
        f'Measurement cache: `{cache.name}`. The script writes generated files under `output/`. Git ignores these files.',
    ]
    figures = []
    captions = {
        'recovery_winrate':'Win rate after ordinary death and continued play, by rank.',
        'recovery_outcomes':'Wins with and without a completion after death and continued play.',
        'death_rank_deficit':'Win rate by rank and opponent split lead at time of death.',
        'recovery_deficit':'Win rate by opponent split lead at time of death.',
        'recovery_stage':'Win rate by the highest split recorded before death.',
        'recovery_time':'Win rate by time of death. The last group contains deaths at 24 minutes or later.',
        'recovery_opponent':'Win rate by whether the opponent had an ordinary death before your death.',
        'opponent_after_death':'Opponent death and forfeit rates after your death, in matches without your forfeit.',
        'opponent_after_death_winrate':'Your win rate by opponent events after your death, in matches without your forfeit.',
        'opponent_death_timing':'Your win rate by opponent deaths before and after your death, in matches without your forfeit.',
        'recovery_seasons':'Win rate after ordinary death and continued play, by season.',
        'recovery_elo':'Elo change in the death match after a forfeit or continued play.',
        'recovery_efficiency':'Elo gain per game hour over 10 games. The same-session comparison also uses 10 games.',
        'forfeit_rating':'Forfeit rate and mean Elo for eligible player records.',
        'forfeit_winrate':'Forfeit rate and win rate for eligible player records.',
        'forfeit_pace':'Forfeit rate and mean completion time. This measure includes completed runs only.',
        'forfeit_completion':'Forfeit rate and the share of matches that the player completes.',
        'forfeit_residual':'Actual match score minus the score predicted by the Elo formula.',
        'forfeit_calibration':'Actual and predicted score for the higher rated player. A draw counts as half a win.',
        'forfeit_improvement':'Later Elo gain per 100 games minus the comparison group mean.',
        'forfeit_hour_improvement':'Later Elo gain per game hour minus the comparison group mean.',
        'recovery_improvement':'Earlier continued play rate and later Elo gain per 100 games minus the group mean.',
        'forfeit_pace_improvement':'Earlier forfeit rate and later pace gain minus the group mean.',
        'recovery_pace_improvement':'Earlier continued play rate and later pace gain minus the group mean.',
        'forfeit_death_improvement':'Earlier forfeit rate and later death rate decrease minus the group mean.',
        'forfeit_cross_season_pace':'Pace gain between adjacent seasons minus the group mean. Completion times use shared seed types.',
        'forfeit_cross_season_rating':'Elo percentile change between adjacent seasons minus the group mean.',
        'forfeit_retention':'Previous season forfeit rate and observed return rate in the next season.',
        'death_reset_exclusions':'Excluded intentional spawnpoint deaths per 100 matches. A match can contain multiple deaths.',
        'forfeit_coverage':'Shares of matches with no completion, explicit forfeits, draws, or wins without a forfeit record.',
    }
    leads = {
        'recovery_outcomes':'A win without a completion still counts as a win. This figure separates those wins from completed runs.',
        'recovery_deficit':'The outer groups combine split leads of two or more. A negative lead means that you are ahead.',
        'recovery_stage':'These figures combine ranks. The split describes recorded history before death.',
        'recovery_time':'This figure groups matches by time of death. It does not adjust for Elo, split position, or opponent lead.',
        'recovery_opponent':'This comparison uses opponent deaths before your death. It does not use later events to describe the situation at death.',
        'forfeit_pace':'Read this figure with the completion rate figure. A fast completed-run average can exclude many forfeited attempts.',
        'forfeit_residual':'The next figure checks the Elo prediction against actual scores.',
    }

    def inline_figures(part):
        matches = list(re.finditer(r'\[([^]]+)\]\(([^)]+\.png)\)', part))
        if not matches:
            return part
        blocks = []
        for match in matches:
            label, relative = match.groups()
            image = directory / relative
            csv_path = image.with_suffix('.csv')
            if not image.is_file() or not csv_path.is_file():
                raise FileNotFoundError(f'Missing report figure or CSV: {relative}')
            if relative in figures:
                continue
            figures.append(relative)
            caption = captions[image.stem]
            if relative.startswith('all_seasons/'):
                caption = 'All-season comparison. ' + caption
            lead = leads.get(image.stem, '')
            if relative.startswith('all_seasons/'):
                lead = ''
            blocks.append(f'{lead}\n\n[![{label}]({relative})]({relative})\n\n*Figure {len(figures)}. {caption}* [CSV estimates]({Path(relative).with_suffix(".csv").as_posix()})'.strip())
        # Figure-reference paragraphs contain no other prose.
        return '\n\n'.join(blocks)

    expanded = []
    for part in summary:
        if re.search(r'\[([^]]+)\]\(([^)]+\.png)\)', part):
            # The coverage paragraph also contains a narrative before its figures.
            split = re.split(r'\n\n(?=\[)', part, maxsplit=1)
            if len(split) == 2:
                expanded.extend([split[0], inline_figures(split[1])])
            else:
                expanded.append(inline_figures(part))
        else:
            expanded.append(part)
    md = '\n\n'.join(expanded) + '\n'
    expected_figures = 31 if main else 33
    assert len(figures) == expected_figures, f'Expected {expected_figures} inline figures; got {len(figures)}'
    (directory/'report.md').write_text(md)
    (directory/'summary_data.json').write_text(json.dumps(dict(main_seasons=list(seasons),audits=audits,pooled_recovery=pooled,opponent_after_death=rows('opponent_after_death_winrate'),profile_summary={RANKS[r]:dict(player_seasons=len(v),median_forfeit=statistics.median(x[0] for x in v)) for r,v in profile_groups.items() if v}),indent=2))
    for style, filename in [('Regular','SF-Pro-Text-Regular.otf'),('Bold','SF-Pro-Text-Bold.otf'),('Italic','SF-Pro-Text-RegularItalic.otf'),('Bold Italic','SF-Pro-Text-BoldItalic.otf')]:
        family, font_path = subprocess.check_output(['fc-match','-f','%{family}\n%{file}',f'SF Pro Text:style={style}'],text=True).split('\n',1)
        if family != 'SF Pro Text':
            raise RuntimeError('SF Pro Text must be installed; refusing to substitute another font')
        shutil.copyfile(font_path,directory/filename)
    doc = '''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Win chances after death</title><style>
@font-face{font-family:"SF Pro Text";src:url('SF-Pro-Text-Regular.otf');font-weight:400;font-style:normal}
@font-face{font-family:"SF Pro Text";src:url('SF-Pro-Text-Bold.otf');font-weight:700;font-style:normal}
@font-face{font-family:"SF Pro Text";src:url('SF-Pro-Text-RegularItalic.otf');font-weight:400;font-style:italic}
@font-face{font-family:"SF Pro Text";src:url('SF-Pro-Text-BoldItalic.otf');font-weight:700;font-style:italic}
*{box-sizing:border-box}body{font-family:"SF Pro Text",sans-serif;background:#fffcf0;color:#403e3c;margin:0;padding:48px 32px;font-size:18px}main{max-width:1120px;margin:auto}h1{font-size:42px;line-height:1.2;letter-spacing:-.025em}h2{margin-top:72px;font-size:30px;line-height:1.25}h3{margin-top:56px;font-size:24px}p,li{line-height:1.7}a{color:#205ea6;text-underline-offset:3px}table{border-collapse:collapse;width:100%;margin:32px 0;font-size:15px;line-height:1.5}th,td{border-bottom:1px solid #dad8ce;padding:12px;text-align:left}th{background:#f2f0e5}pre{padding:20px;background:#f2f0e5;overflow:auto}code{font-family:inherit}img{display:block;width:100%;height:auto;margin:32px 0 14px}p:has(img){margin-bottom:0}p:has(>em:first-child){font-size:15px;color:#6f6e69;line-height:1.5;margin-top:8px;margin-bottom:44px}strong{font-weight:700}@media(max-width:700px){body{padding:20px 14px;font-size:16px}table{font-size:12px;display:block;overflow:auto}h1{font-size:32px}h2{font-size:26px}img{margin-top:24px}}@media print{body{padding:0;background:white}main{max-width:none}h2,h3{break-after:avoid}p:has(img),table{break-inside:avoid}a{color:inherit}img{max-height:240mm;object-fit:contain}}
</style></head><body><main>''' + markdown.markdown(md,extensions=['tables','fenced_code']) + '</main></body></html>'
    (directory/'index.html').write_text(doc)
    print(f'Wrote report.md and index.html with {len(figures)} inline figures',flush=True)
    return directory/'index.html'
