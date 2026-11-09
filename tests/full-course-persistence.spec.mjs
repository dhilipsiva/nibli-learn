import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const course = JSON.parse(await readFile(new URL('../content/course.json', import.meta.url), 'utf8'));
const releaseLesson = 'found.investigate-release';
const activities = course.lessons.flatMap(lesson => lesson.activities.map((activity, index) => ({ lesson, activity, index })));

async function ready(page) {
  await expect(page.getByTestId('worker-status')).toHaveAttribute('data-state', 'ready');
}

async function saved(page) {
  await expect(page.getByRole('button', { name: 'Saved on this device', exact: true })).toBeVisible();
  return page.evaluate(async () => {
    const platform = await import('/nibli-learn/platform/platform.js');
    const result = JSON.parse(await platform.loadWorkspace());
    if (!result.ok || !result.value) throw new Error(result.error?.message || 'Workspace has not been committed');
    return result.value.data;
  });
}

test('the whole course retains every activity project and earlier completed evidence after reload', async ({ page }, testInfo) => {
  test.setTimeout(300_000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  let firstProject;
  let firstProgress;
  let finalWorkspace;
  const visited = [];
  for (const { lesson, activity, index } of activities) {
    await test.step(`${lesson.unit}: ${activity.id}`, async () => {
      await page.goto('/nibli-learn/#/course');
      await ready(page);
      await page.getByTestId(`activity-${activity.id}`).click();
      await ready(page);
      if (lesson.id === releaseLesson) {
        await expect(page.getByTestId(`step-${index + 1}`)).toHaveAttribute('aria-current', 'step');
      } else {
        await expect(page.getByRole('heading', { level: 1, name: activity.title, exact: true })).toBeVisible();
      }
      if (visited.length === 0) {
        await page.getByLabel('Your prediction or diagnosis').fill('The missing approval must be admitted before the cart is ready.');
        await page.getByTestId('activity-run').click();
        await ready(page);
        await page.getByTestId('action-admit-approval').click();
        await ready(page);
        await page.getByRole('radio').nth(activity.question.correct).check();
        await page.locator('.self-review input[type=checkbox]').check();
        await page.getByTestId('check-activity').click();
        await expect(page.getByTestId('check-activity')).toHaveText('✓ Activity checked');
      }
      finalWorkspace = await saved(page);
      expect(finalWorkspace.progress.current).toEqual({ lesson: lesson.id, activity: activity.id });
      const project = finalWorkspace.projects[finalWorkspace.active_project];
      expect(project.lesson).toBe(lesson.id);
      expect(project.activity).toBe(activity.id);
      if (visited.length === 0) {
        firstProject = project;
        firstProgress = finalWorkspace.progress.lessons[lesson.id][lesson.version][activity.id][activity.version];
        expect(firstProgress.checked).toBe(true);
        expect(firstProgress.evidence).toHaveLength(2);
      }
      visited.push(activity.id);
    });
  }
  // The seven original release steps intentionally share one investigation.
  const expectedProjects = activities.filter(({ lesson }) => lesson.id !== releaseLesson).length + 1;
  expect(Object.keys(finalWorkspace.projects)).toHaveLength(expectedProjects);
  expect(expectedProjects).toBeGreaterThan(64);
  expect(finalWorkspace.projects[firstProject.id]).toEqual(firstProject);
  await page.reload();
  await ready(page);
  const restored = await saved(page);
  expect(restored.projects).toEqual(finalWorkspace.projects);
  expect(restored.progress).toEqual(finalWorkspace.progress);
  await page.goto('/nibli-learn/#/course');
  await page.getByTestId(`activity-${activities[0].activity.id}`).click();
  await ready(page);
  await expect(page.getByTestId('check-activity')).toHaveText('✓ Activity checked');
  const earliest = await saved(page);
  expect(earliest.projects[firstProject.id]).toEqual(firstProject);
  const { lesson, activity } = activities[0];
  expect(earliest.progress.lessons[lesson.id][lesson.version][activity.id][activity.version]).toEqual(firstProgress);
  expect(errors).toEqual([]);
  await testInfo.attach('full-course-persistence.json', {
    body: JSON.stringify({ visited, saved_projects: expectedProjects, retained_earliest_project: firstProject.id, retained_earliest_evidence: firstProgress.evidence.length, serialized_bytes: Buffer.byteLength(JSON.stringify(restored)), reload_verified: true }, null, 2),
    contentType: 'application/json',
  });
});
